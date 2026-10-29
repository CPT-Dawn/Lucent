# Idle exit by default

Lucent exits shortly after its Stack and Queue become empty and relies on D-Bus activation to start it again for the next Notification. This makes the idle footprint literally zero (no process, no memory, no wakeups), which no mainstream notification daemon offers, at the cost of a cold start on the first Notification after idle. Users can opt out with a config switch to stay resident.

## Consequences

- Nothing may live only in memory across Notifications: no history, no do-not-disturb toggle held in RAM. Any such feature must persist to disk or be rejected.
- Notification ids must not restart at 1 on every launch, or a client holding an old id could replace or close an unrelated Notification; the next id is persisted (e.g. under `$XDG_RUNTIME_DIR`).
- Only the process that owns the bus name holds Notifications or hands out ids. A Notify that was already routed to an exiting Lucent when it released the name is forwarded to the name's next owner, and its reply relayed, so the Notification is not lost, its signals come from the name owner (which clients filter on), and two processes never hand out ids at once.
- Cold-start latency is a tracked benchmark number, not an afterthought.
