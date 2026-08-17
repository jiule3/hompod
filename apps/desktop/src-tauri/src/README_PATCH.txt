HomePod Bridge Low-Latency Patch

Changes:
1. Updates cap-core to AirSend commit ee55c8d5d2aa15959bd7e936a73fe9ff7d487758.
2. Maps HomePod Bridge latency modes to the real receiver-side AirPlay profiles:
   Ultra  -> Gaming (~250 ms to 1 s)
   Low    -> Video  (~350 ms to 2 s)
   Stable -> Music  (~500 ms to 3 s)
3. Ultra mode no longer silently falls back to Stable after reconnect failures.

Upload the contents of this patch over the same paths in your repository root.
