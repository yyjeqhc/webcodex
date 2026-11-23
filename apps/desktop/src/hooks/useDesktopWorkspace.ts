import { useCallback, useEffect, useRef, useState } from "react";
import { subscribeDesktopNavigation } from "../lib/desktop-navigation";
import { desktopApi } from "../lib/desktop-api";
import { desktopIdentity, type ChatgptActivityQueryStatus } from "../lib/desktop-observation";
import { isErrorStaleForState, reconcileDesktopStateErrors } from "../lib/desktop-errors";
import type { ActivityEntry, DesktopError, DesktopState } from "../models/topology";
import { normalizeDesktopError } from "../i18n/presentation";
import { NAVIGATION, type Navigation } from "../components/Sidebar";

const DESKTOP_OBSERVATION_INTERVAL_MS = 1_500;
const CHATGPT_ACTIVITY_OBSERVATION_INTERVAL_MS = 30_000;
const ACTIVE_OPERATION_OBSERVATION_INTERVAL_MS = 1_000;

export function useDesktopWorkspace() {
  const [state, setState] = useState<DesktopState | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [{navigation, previousPage}, setPage] = useState<{navigation: Navigation; previousPage?: Navigation}>({navigation: "home"});
  const selectPage = useCallback((page: Navigation) => {
    setPage(current => current.navigation === page ? current : { navigation: page, previousPage: current.navigation });
  }, []);
  const [refreshing, setRefreshing] = useState(false);
  const [runtimeObservations, setRuntimeObservations] = useState(0);
  const [error, setError] = useState<DesktopError | null>(null);
  const [cancelSubmittingId, setCancelSubmittingId] = useState<string | null>(null);
  const [setupOrigin, setSetupOrigin] = useState<"home" | "settings" | "connection" | null>(null);
  const showSetup = setupOrigin !== null;
  const setNavigation = useCallback((page: Navigation) => {
    setSetupOrigin(null);
    selectPage(page);
  }, [selectPage]);
  const [startupAttempt, setStartupAttempt] = useState(0);
  const [windowFocused, setWindowFocused] = useState(true);
  const [chatgptActivityQuery, setChatgptActivityQuery] = useState<{ identity: string; status: ChatgptActivityQueryStatus } | null>(null);
  const latestGeneration = useRef(0);
  const stateVersionRef = useRef(0);
  const activeIdentityRef = useRef<string | null>(null);
  const errorIdentityRef = useRef<string | null>(null);
  const mainRef = useRef<HTMLElement>(null);
  const hasCurrentOperation = Boolean(state?.current_operation);
  const hasLoadedState = Boolean(state);
  const shouldObserveChatgptActivity = Boolean(
    state?.readiness.runtime_ready
      && state?.project?.runtime_project_id
      && !hasCurrentOperation
      && !refreshing
      && windowFocused,
  );
  const chatgptActivityIdentity = state ? desktopIdentity(state) : null;
  const chatgptActivityQueryStatus: ChatgptActivityQueryStatus = chatgptActivityQuery?.identity === chatgptActivityIdentity
    ? chatgptActivityQuery.status === "pending" && !shouldObserveChatgptActivity ? "idle" : chatgptActivityQuery.status
    : "idle";

  useEffect(() => {
    const onFocus = () => setWindowFocused(true);
    const onBlur = () => setWindowFocused(false);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  useEffect(() => {
    mainRef.current?.focus({ preventScroll: true });
    mainRef.current?.scrollTo?.({ top: 0 });
    // Narrow layouts scroll the window rather than the main pane.
    if (window.innerWidth <= 600) window.scrollTo(0, 0);
  }, [navigation, showSetup]);

  useEffect(() => subscribeDesktopNavigation((target) => {
    setNavigation(target === "connections" ? "connection" : target);
  }), [setNavigation]);

  useEffect(() => {
    const navigateWithKeyboard = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.repeat) return;
      // Ctrl/⌘+, is the conventional desktop shortcut for preferences.
      const page = event.key === "," ? "settings" : NAVIGATION[Number(event.key) - 1];
      if (!page) return;
      event.preventDefault();
      setNavigation(page);
    };
    window.addEventListener("keydown", navigateWithKeyboard);
    return () => window.removeEventListener("keydown", navigateWithKeyboard);
  }, [setNavigation]);

  const openSetup = () => {
    setSetupOrigin(navigation === "settings" && previousPage === "connection" ? "connection" : navigation === "settings" ? "settings" : "home");
    selectPage("home");
  };

  const closeSetup = () => setNavigation(setupOrigin ?? "home");

  const commitState = useCallback((next: DesktopState) => {
    if (next.state_generation !== undefined) {
      if (next.state_generation < latestGeneration.current) return;
      latestGeneration.current = next.state_generation;
    }
    stateVersionRef.current += 1;
    const reconciled = reconcileDesktopStateErrors(next);
    const nextIdentity = desktopIdentity(reconciled);
    activeIdentityRef.current = nextIdentity;
    setState(reconciled);
    setError((current) => {
      if (!current) return null;
      return isErrorStaleForState(current, reconciled, errorIdentityRef.current, nextIdentity)
        ? null
        : current;
    });
  }, []);

  const recordError = useCallback((value: unknown, expectedIdentity?: string | null) => {
    if (
      expectedIdentity
      && activeIdentityRef.current
      && expectedIdentity !== activeIdentityRef.current
    ) {
      return;
    }
    errorIdentityRef.current = activeIdentityRef.current;
    setError(normalizeDesktopError(value));
  }, []);

  const observeFailedOperation = useCallback(async (value: unknown, current: () => boolean = () => true) => {
    if (!current()) return;
    const observedIdentity = activeIdentityRef.current;
    recordError(value, observedIdentity);
    const observedVersion = stateVersionRef.current;
    try {
      const terminal = await desktopApi.getState();
      if (current() && stateVersionRef.current === observedVersion) commitState(terminal);
    } catch {
      // Preserve the original failure if this best-effort observation fails.
    }
  }, [commitState, recordError]);

  const commitChatgptActivity = useCallback((next: DesktopState) => {
    stateVersionRef.current += 1;
    setState((current) => {
      if (!current) return next;
      if (desktopIdentity(current) !== desktopIdentity(next)) return current;
      if (current.chatgpt_activity?.observed && !next.chatgpt_activity?.observed) return current;
      return { ...current, chatgpt_activity: next.chatgpt_activity };
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let bootstrapSettled = false;
      try {
        const initial = await desktopApi.getState();
        if (cancelled) return;
        if (initial.current_operation || initial.configuration_issue) {
          commitState(initial);
          bootstrapSettled = true;
          return;
        }

        // Fresh setup presents local defaults. Reading state
        // must never enroll a Runner or install/start services on mount.
        commitState(initial);
        if (!initial.topology) {
          bootstrapSettled = true;
          return;
        }

        // Once the native process has completed its one startup bootstrap, every
        // replacement renderer is observation-only. Check this before even the
        // persistent-Environment eager refresh so recreation cannot overlap the
        // no-window native observer or replay startup work.
        if (await desktopApi.shellRestoreOnly()) {
          bootstrapSettled = true;
          return;
        }
        if (cancelled) return;
        if (initial.persistent_environment) {
          // System services own persistence. Opening Desktop only observes them;
          // it must not restart a service the user explicitly stopped.
          setRefreshing(true);
          try {
            const observed = await desktopApi.refresh();
            if (!cancelled) commitState(observed);
          } catch (value) {
            if (!cancelled) recordError(value);
          } finally {
            if (!cancelled) setRefreshing(false);
          }
          bootstrapSettled = true;
          return;
        }
        const resumeExisting = Boolean(
          initial.runtime_autostart
          && initial.topology.experience === "full",
        );
        const resumeConnections = !resumeExisting
          && initial.topology.experience === "full"
          && initial.topology.server.kind === "local"
          && initial.readiness.runtime_ready
          && !initial.quick_share
          && initial.connections?.profiles.some(profile => profile.enabled && profile.autostart && !profile.process_started);
        if (!resumeExisting && !resumeConnections) {
          bootstrapSettled = true;
          return;
        }

        setRefreshing(true);
        try {
          const next = resumeExisting
            ? await desktopApi.resumeSavedRuntime()
            : await desktopApi.resumeSavedConnections();
          if (cancelled) return;
          // Backend reconciliation owns every profile's autostart policy.
          commitState(next);
        } catch (value) {
          await observeFailedOperation(value, () => !cancelled);
        } finally {
          if (!cancelled) setRefreshing(false);
        }
        bootstrapSettled = true;
      } catch (value) {
        if (!cancelled) recordError(value);
      } finally {
        if (!cancelled && bootstrapSettled) {
          try {
            await desktopApi.shellBootstrapComplete();
          } catch {
            // Shell readiness is an optimization gate. Keep ordinary Desktop
            // usable if this best-effort native acknowledgement is unavailable.
          }
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [commitState, observeFailedOperation, startupAttempt]);
  useEffect(() => {
    if (!hasLoadedState) return;

    let cancelled = false;
    let timeoutId: number | undefined;
    const interval = hasCurrentOperation || refreshing
      ? ACTIVE_OPERATION_OBSERVATION_INTERVAL_MS
      : DESKTOP_OBSERVATION_INTERVAL_MS;

    const scheduleObservation = () => {
      timeoutId = window.setTimeout(() => {
        void (async () => {
          const observedVersion = stateVersionRef.current;
          const refreshRuntime = Boolean(state?.persistent_environment && !hasCurrentOperation && !refreshing);
          // Keep this separate from refreshing: changing that effect dependency
          // would cancel this observation before it can commit its result.
          if (refreshRuntime) setRuntimeObservations(count => count + 1);
          try {
            const next = refreshRuntime
              ? await desktopApi.refresh()
              : await desktopApi.getState();
            if (!cancelled && stateVersionRef.current === observedVersion) {
              commitState(next);
            }
          } catch {
            // Observation is best-effort. A transient invoke failure must not
            // create an error storm or a second concurrent observer.
          } finally {
            if (refreshRuntime) setRuntimeObservations(count => count - 1);
            if (!cancelled) scheduleObservation();
          }
        })();
      }, interval);
    };

    scheduleObservation();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, [commitState, hasCurrentOperation, hasLoadedState, refreshing, state?.persistent_environment]);

  useEffect(() => {
    if (!shouldObserveChatgptActivity || !chatgptActivityIdentity) return;

    const identity = chatgptActivityIdentity;
    let cancelled = false;
    let timeoutId: number | undefined;
    const current = () => !cancelled && activeIdentityRef.current === identity;
    const observe = async () => {
      if (!current()) return;
      setChatgptActivityQuery({ identity, status: "pending" });
      try {
        const next = await desktopApi.observeChatgptActivity();
        if (current()) {
          if (desktopIdentity(next) === identity) commitChatgptActivity(next);
          setChatgptActivityQuery({ identity, status: "idle" });
        }
      } catch {
        // A failed read is not an empty activity history. Keep it scoped to the
        // same runtime identity and preserve any previously observed evidence.
        if (current()) setChatgptActivityQuery({ identity, status: "failed" });
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(
            () => void observe(),
            CHATGPT_ACTIVITY_OBSERVATION_INTERVAL_MS,
          );
        }
      }
    };

    void observe();
    return () => {
      cancelled = true;
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, [chatgptActivityIdentity, commitChatgptActivity, shouldObserveChatgptActivity]);

  useEffect(() => {
    if (navigation !== "activity" && !(navigation === "home" && hasLoadedState && state?.topology)) return;
    let current = true;
    void desktopApi.activity().then(entries => { if (current) setActivity(entries ?? []); }).catch(() => undefined);
    return () => { current = false; };
  }, [navigation, state?.activity_sequence, hasLoadedState, Boolean(state?.topology)]);

  const runStateOperation = async (operation: () => Promise<DesktopState>) => {
    setError(null);
    try {
      commitState(await operation());
    } catch (value) {
      await observeFailedOperation(value);
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    setError(null);
    try {
      commitState(await desktopApi.refresh());
    } catch (value) {
      const failure = normalizeDesktopError(value);
      if (failure.code === "desktop_operation_busy") {
        try {
          commitState(await desktopApi.getState());
        } catch {
          // The competing operation remains authoritative; a failed observation
          // must not turn an ordinary Refresh race into a second error.
        }
      } else {
        await observeFailedOperation(value);
      }
    } finally {
      setRefreshing(false);
    }
  };

  const resumeRuntime = async () => {
    setRefreshing(true);
    setError(null);
    try {
      commitState(await desktopApi.resumeSavedRuntime());
    } catch (value) {
      await observeFailedOperation(value);
    } finally {
      setRefreshing(false);
    }
  };

  const cancelCurrentOperation = async () => {
    const observed = state?.current_operation;
    if (!observed || !observed.cancellable || observed.phase === "cancelling") return;
    setCancelSubmittingId(observed.id);
    setError(null);
    try {
      commitState(await desktopApi.cancelOperation(observed.id));
    } catch (value) {
      recordError(value);
    } finally {
      setCancelSubmittingId((current) => current === observed.id ? null : current);
    }
  };

  return { state, activity, chatgptActivityQueryStatus, navigation, previousPage, setNavigation, refreshing: refreshing || runtimeObservations > 0, preserveWorkspacePollDeadline: runtimeObservations > 0 && !refreshing, error, setError, cancelSubmittingId, showSetup, setupOrigin, closeSetup, setStartupAttempt, mainRef, commitState, openSetup, refresh, resumeRuntime, cancelCurrentOperation, runStateOperation };
}
