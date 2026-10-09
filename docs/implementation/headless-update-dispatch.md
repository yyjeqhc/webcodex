# Restore the headless update entry point

At upstream commit `4700beb15faa6f8483ece8b93d41b204bccdd80f`, the CLI's
Environment adapter had lost the `update` dispatch and its help string used
literal backslash-n text. Existing tests inside the updater did not exercise
the public Environment entry point.

Three new public-entry tests use isolated child-local config/cache roots and
stdin EOF. Before the fix, all three failed: update status returned generic
Environment help, nonTTY apply did not return the updater's structured admission
response, and help did not contain real newlines. After restoring dispatch to
`update::run` and correcting the help escapes, the same three tests passed.

The adapter reuses existing update parsing, TTY/confirmation admission, bounded
status projection and transactions. No updater, service or installer behavior
was added. Focused dogfood tests, CLI build and built-binary checks cover absent
roots and nonTTY effect refusal; they do not establish native installation,
reboot, authorization or rollback acceptance.
