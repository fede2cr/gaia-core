All of the containers will be managed via podman, built in CI and sent to docker hub. Each project will have a compose.yaml file and an associated systemd service to run it. All containers will be built for ARM64 as well as AMD64 in GitHub CI.

All of the applications should use Rust as the primary source, and all web application will use the Web framework in Rust called Leptos.