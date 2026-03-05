//! Live MJPEG camera streaming for the GMN camera pre-alignment view.
//!
//! Spawns `ffmpeg` to capture from a V4L2 device and outputs an MJPEG
//! multipart stream that any browser `<img>` tag can display natively.

use axum::{
    body::Body,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_stream::wrappers::ReceiverStream;

/// Query parameters for the camera stream endpoint.
#[derive(Deserialize)]
pub struct StreamParams {
    /// V4L2 device path, e.g. `/dev/video0`.
    pub device: String,
}

/// `GET /api/camera-stream?device=/dev/video0`
///
/// Returns an MJPEG multipart stream (`multipart/x-mixed-replace`).
/// The stream ends when the client disconnects (the ffmpeg process is
/// killed automatically via `kill_on_drop`).
pub async fn camera_stream(Query(params): Query<StreamParams>) -> Response {
    let device = &params.device;

    // Validate: must look like /dev/videoN
    if !device.starts_with("/dev/video") {
        return (StatusCode::BAD_REQUEST, "Invalid device path").into_response();
    }

    // Check the device file exists.
    if tokio::fs::metadata(device).await.is_err() {
        return (
            StatusCode::NOT_FOUND,
            format!("Device {device} not found -- is it mounted into the container?"),
        )
            .into_response();
    }

    // Spawn ffmpeg: V4L2 → MJPEG multipart on stdout.
    let child = Command::new("ffmpeg")
        .args([
            "-f",
            "v4l2",
            "-video_size",
            "640x480",
            "-framerate",
            "10",
            "-i",
            device,
            "-f",
            "mpjpeg",
            "-boundary_tag",
            "gaia",
            "-q:v",
            "8",
            "-an",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to spawn ffmpeg: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to start camera stream: {e}"),
            )
                .into_response();
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "No stdout from ffmpeg",
            )
                .into_response();
        }
    };

    // Channel-based stream: the spawned task owns the child process and
    // reads chunks from ffmpeg's stdout.  When the receiver (HTTP body)
    // is dropped (client disconnects), the sender fails and the task
    // exits, dropping the child and killing ffmpeg.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(8);

    tokio::spawn(async move {
        let _child = child; // keep alive; kill_on_drop triggers on drop
        let mut stdout = stdout;
        let mut buf = vec![0u8; 65_536];

        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(Ok(buf[..n].to_vec())).await.is_err() {
                        break; // receiver dropped -- client disconnected
                    }
                }
                Err(e) => {
                    tracing::debug!("Camera stream read error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("Camera stream ended for {}", params.device);
    });

    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    Response::builder()
        .header(
            "Content-Type",
            "multipart/x-mixed-replace;boundary=gaia",
        )
        .header("Cache-Control", "no-cache, no-store, must-revalidate")
        .header("Pragma", "no-cache")
        .body(body)
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build response").into_response()
        })
}
