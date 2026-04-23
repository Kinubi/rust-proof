# rp-runtime

`rp-runtime/` is the desktop or server runtime crate.

Its target role is:

- host the shared `rp-node` engine
- provide transport, storage, timers, and process lifecycle integration
- stay thin and avoid owning canonical node logic

This crate is currently only a skeleton. The real implementation work is still ahead.