# Local protocol v1 (experimental)

One request and one response per Unix connection. A frame is a big-endian u32 byte
length followed by JSON, limited to 256 KiB before allocation. Requests require
`version: 1` and a tagged `operation`. Unknown fields and versions are rejected.
Input data uses integer arrays in this bootstrap protocol; high-throughput binary
frames and negotiated capabilities belong to the later remote protocol.

Operations: status, read(after byte offset), acquire(takeover), input(generation,
sequence,data), resize(generation,cols,rows), release(generation), stop.

Input batches are at most 16 KiB. Dimensions range from 1 to 1000 on either axis.
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
permissions are the current trust boundary. No network or third-party clients should
be granted access before the identity/transport release gate is met.
