# Claude native channel proof

Observed by Codex root on 2026-09-11, approximately 07:10–07:12 UTC. This proves the native Claude channel mechanism, not the Aoide mail integration.

Environment: Claude Code 2.1.260, interactive PTY, disposable doorbell-proof project. Launched with `claude --dangerously-load-development-channels server:probe`. Accepted the reviewed local project, only its probe MCP server, and the development-channel startup dialog. The banner stated that messages from server:probe inject directly into this session. Existing user and Fable composers were not touched.

The probe is the local zero-dependency stdio MCP server prepared by Fable. It declares experimental claude/channel and forwards a Unix-socket payload to notifications/claude/channel. The initialize request identified Claude Code 2.1.260. Socket messages were sent using Python because socat was unavailable.

| Test | Observation | Result |
|---|---|---|
| Idle, empty composer | After READY completed, a socket-only IDLE-PROBE-0711 event started a new response and produced ACK IDLE-PROBE-0711. No terminal input was sent to initiate that response. | Passed |
| Idle, unsent draft | UNSENT-DRAFT-KEEP-0711 remained in the composer while a socket-only DRAFT-PROBE-0711 event produced ACK DRAFT-PROBE-0711. A subsequent Ctrl-L redraw confirmed the exact draft remained. | Passed |
| Busy response | Sent BUSY-PROBE-0711 during generation of integers 1–120. The response reached BUSY-DONE, then a separate response acknowledged BUSY-PROBE-0711. | Passed for this text-generation case |

The draft was later deliberately submitted as part of the busy-test prompt: Ctrl-U did not remove it in this editor mode. This occurred after draft preservation was verified and was not caused by the channel event.

Evidence: root tool PTY session 37748; timestamped probe.log under `/tmp/claude-1000/-home-khoa-Aoide/e7fce841-5c0e-4d6b-b9e5-f86d4f8c6f0d/scratchpad/doorbell-proof/`. PTY output was inspected directly, not through desktop screenshots. The normal installed startup/stop hooks also ran; no tools or subagents were requested from the test model.

Remaining acceptance: Aoide channel registration/routing, mailbox read and latch semantics, duplicate suppression, reconnect/relaunch, unavailable-channel behavior, busy tool execution, and graphical terminal verification. Do not label production mail doorbells working from this proof alone.
