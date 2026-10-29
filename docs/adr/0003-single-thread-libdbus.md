# Single thread, libdbus, no async runtime

v2 uses the synchronous `dbus` crate (libdbus) and polls its connection fd from the same `calloop` event loop that drives Wayland, so Lucent runs on exactly one thread with no async executor. `zbus` is the idiomatic Rust choice and was used in v1, but even without tokio it runs an internal executor thread; libdbus is already present on every target system, so the extra C dependency costs nothing resident. Do not reintroduce tokio or zbus for convenience: the thread count and idle wakeups are part of the product's benchmark.
