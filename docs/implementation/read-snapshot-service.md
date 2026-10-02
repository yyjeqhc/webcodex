# File-read snapshot ownership

`ReadCache` owns snapshot retention, singleflight and per-waiter deadlines.
Its shared physical work captures only the cache and `ProjectFileReader`, not a
clone of `ToolRuntime`. `ProjectFileReader` has one private dependency: the
Runner registry needed to enqueue target-fenced reads, validate bounded replies
and cancel abandoned requests. The outer `read_files` integration composes
these capabilities; it still owns authorization and read-revision publication.

The boundary is intentionally narrow. It does not combine unrelated Runtime
fields into a context or remove repeated authority/target validation. Cache hits
still obtain a fresh full-file SHA from the owning Runner. Session/authority,
Project root/fingerprint and Runner instance continue to partition retained
content. Unscoped reads remain uncached. Physical work and individual waiters
retain their distinct absolute deadlines, weak singleflight ownership and
last-waiter cancellation.

A lifetime regression starts a real in-process Runner request through only the
cache/reader service, drops the composing Runtime and proves unrelated revision
state is released while the read completes. Existing read-cache/read-files
coverage retains SHA-change, instance replacement, scope partition, late-waiter,
cancellation, range and result-budget checks. No protocol or filesystem behavior
is changed; no new crate or general service framework is introduced.
