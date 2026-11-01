# Local Computer Control

Windows Desktop shows Computer activity in its native tray tooltip. On a saved environment
with a local Runner, **Pause Computer Control** blocks further native input for all
remote clients using that Runner. **Resume Computer Control** requires an explicit
local action. The main Desktop window can be closed while the tray and Runner remain
running.

The status distinguishes observation, control requests, idle, paused control,
paused control with observation, and pausing while an admitted control request is
still in flight. Short requests leave a separate recent-activity indication for
1.5 seconds. Unavailable local state is displayed as unavailable.

Pausing covers pointer movement/clicks, keyboard and text input, window activation,
application launch, clipboard writes and semantic control actions. Observation
permissions remain unchanged, including screenshots, accessibility/window queries
and clipboard reads. A pause does not undo an operation that already entered native
execution. Resume stays disabled until admitted controls finish.

The tray menu contains a single Computer action: pause, or resume after pausing.
The screen-edge hint appears automatically during control activity and for 1.5
seconds afterward. Pressing a physical **Esc** key during that activity pauses
control; it never resumes control. Injected keys, including Computer Escape input,
do not trigger the shortcut. Escape still reaches the foreground application.
Observation alone does not enable the shortcut or edge hint. The edge hint is placed
on up to sixteen connected displays without activation or pointer interception and
requests Windows capture exclusion. Actual capture exclusion and full-screen
appearance depend on the Windows compositor and capture provider.

## Execution and recovery

`webcodex-computer-control` owns the local gate and bounded management protocol;
`webcodex-computer` retains native target validation and Browser handoff reservations.
The Runner and its interactive helper use the same gate for request admission and
native effect entry. Resume never releases a Browser user handoff.

An admitted request is still revocable while it waits for an input lock or native
target preflight. The same pause/epoch guard is checked again after the lock and
at the first native effect commit point. A rejected commit performs no input and
does not consume a pointer snapshot generation. A sequence already committed may
finish its key releases and native cleanup; its original success or uncertain
outcome is retained.

The additive `computer_local_control` capability declares software support even
before an interactive helper is ready; control waits for its first epoch.
The Server stamps control requests with the Runner's advertised local control
epoch. Admission, queued dispatch and native execution preserve that version.
Pause/resume, Runner replacement and helper session invalidation retire older
control requests. They are rejected rather than replayed, including clipboard
writes that do not carry a window or element handle. New control requires a fresh
request after local resume and observation of current desktop state.

On push transports, reading the current control epoch, enqueueing its metadata,
and recording the successful publication share one critical section with
registration resets. Concurrent heartbeat and result publishers cannot leave an
older epoch queued after a newer epoch while marking the newer one as reported.
Failed enqueue attempts retain the previous marker so publication can retry.

The pause setting is stored independently of opaque Computer IDs. Restarting the
Runner or helper preserves a confirmed pause and creates a fresh control epoch.
Malformed saved state prevents enabling input. A failed pause persistence attempt
leaves the in-memory gate closed and reports an error; a retry must persist before
it can confirm success. The gate retains that failure in local status until a
successful save. Desktop shows that further input is blocked but the pause was
not saved, warns that a restart may enable control, and offers **Retry saving
Computer Control pause** instead of Resume. Status polling and Desktop reconnects
do not acknowledge an unsaved pause. Local status without the persistence-failure
field is unavailable rather than assumed durable; update Desktop and Runner together.

The Windows management pipe accepts only status/pause/resume, is scoped to the
interactive session, rejects remote pipe clients, and grants access to its creating
account and LocalSystem. Desktop pins the server executable and the observed epoch.
Management is independent of the serialized native-operation pipe. It carries no
credentials, screenshots, window titles, typed text or clipboard contents. There is
no remote Computer tool for resuming the gate. This is a gate for WebCodex
Computer operations, not an operating-system sandbox for separately authorized
Shell/script execution. Code running as the same interactive account could still
use native input APIs or access the account-scoped management pipe; the local
pause must not be presented as a security boundary against such code.

Update Desktop, Runner and Server together. An updated managed Windows Runner
rejects unfenced control from an older Server while retaining the observation
contract. Older Runners retain their existing behavior and cannot provide this
local pause guarantee. The shared state machine is platform neutral; the local
management transport and Desktop controls currently require Windows.

An unmanaged Windows Runner started without a Computer Helper in noninteractive
Session 0 continues to serve Shell, file and Job tools, but keeps all native
Computer effects disabled. Without an interactive desktop there is no local
management pipe or Resume entrypoint. Other pipe initialization failures remain
fatal rather than silently enabling Computer input.

The focused native pipe test uses disposable local state and checks server identity,
pause/resume, stale epochs, duplicate ownership and client disconnect recovery.
It does not establish real Desktop tray rendering, shortcut registration,
multi-monitor/full-screen behavior or capture exclusion acceptance.
