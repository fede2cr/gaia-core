// Diagnostic script: report hydration lifecycle to the server.
// Runs in <head> before the WASM module import (which uses requestIdleCallback).
(function() {
    var base = '/api/hydrate-ping?';
    function ping(params) {
        try { fetch(base + params).catch(function(){}); } catch(e) {}
    }

    // Confirm the browser executes scripts at all.
    ping('phase=head-script');

    // Catch synchronous JS errors.
    window.addEventListener('error', function(e) {
        ping('phase=js-error&msg=' + encodeURIComponent(
            (e.message || '') + ' at ' + (e.filename || '') + ':' + (e.lineno || '')
        ));
    });

    // Catch unhandled promise rejections (e.g., WASM module import failures).
    window.addEventListener('unhandledrejection', function(e) {
        ping('phase=promise-reject&msg=' + encodeURIComponent(
            String(e.reason || '').substring(0, 500)
        ));
    });
})();
