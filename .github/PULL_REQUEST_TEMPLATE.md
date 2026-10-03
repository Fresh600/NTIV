## Summary

Describe the user-visible change and why it is needed.

## Verification

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Manually tested in Windows Terminal when UI behavior changed

## Safety

- [ ] File operations remain recoverable or require confirmation
- [ ] The terminal is restored on exit and errors
