# Local protocol v1 (experimental)

One request and one response per Unix connection. A frame is a big-endian u32 byte
length followed by JSON, limited to 256 KiB before allocation. Requests require
`version: 1` and a tagged `operation`. Unknown fields and versions are rejected.
Input data uses integer arrays in this bootstrap protocol; high-throughput binary
frames and negotiated capabilities belong to the later remote protocol.

Operations: status, screen, read(after byte offset), acquire(takeover), input(generation,
sequence,data), resize(generation,cols,rows), release(generation), stop.

Input batches are at most 16 KiB. Dimensions range from 1 to 240 columns and 1 to 100 rows.
Read pages are at most 16 KiB. Each keeper retains at most 1 MiB of output in RAM;
`gap` indicates an offset older than retained history. Offsets ahead of the current
stream are rejected. Byte history is not a terminal screen snapshot.

Acquire assigns a monotonically increasing controller generation. Explicit takeover
invalidates prior generations. There is no automatic lease expiry yet. The session
UUID scopes the generation to one keeper lifetime; an old ID is never reused.

A controller sends input sequences 1,2,3,... . Repeating the latest acknowledged
sequence with identical data returns duplicate=true. Older or conflicting requests
are refused. While an input write is blocked, control mutations return busy, and
status/output remain available. A failed/partial write marks the controller's input
outcome uncertain until an explicit takeover. The client does not automatically retry.

The protocol does not authenticate different principals of the same OS user. File
permissions are the current trust boundary. The development USB bridge gives one phone a random, revocable capability to
one keeper through a loopback-only listener. This is not the production remote
identity/transport protocol; do not expose it to a network.

`screen` returns dimensions, visible lines, cursor position and exit state from
owner-side Alacritty VT state. It is a complete monochrome snapshot with no byte
offset coupling. Color/style/mode metadata and history navigation are future wire
extensions. Combining sequences are capped at eight marks per cell; a snapshot
exceeding the frame budget returns a clear error. Clipboard/title events and
terminal response requests are currently ignored by the adapter. This limits TUI
compatibility; shell command entry is the supported Android development workload.
