import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { desktopApi } from "../lib/desktop-api";
import type { ActivityEntry, DesktopError, DesktopState } from "../models/topology";
import { useLocale } from "../i18n/locale";
import { normalizeDesktopError } from "../i18n/presentation";
import { NAVIGATION, type Navigation } from "../components/Sidebar";

const DESKTOP_OBSERVATION_INTERVAL_MS = 1_500;
const CHATGPT_ACTIVITY_OBSERVATION_INTERVAL_MS = 30_000;
const ACTIVE_OPERATION_OBSERVATION_INTERVAL_MS = 1_000;

export function useDesktopWorkspace() {
  const { t } = useLocale();
  const [state, setState] = useState<DesktopState | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [navigation, setNavigation] = useState<Navigation>("home");
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [cancelSubmittingId, setCancelSubmittingId] = useState<string | null>(null);
  const [showSetup, setShowSetup] = useState(false);
  const [startupAttempt, setStartupAttempt] = useState(0);
  const [windowFocused, setWindowFocused] = useState(true);
  const stateVersionRef = useRef(0);
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

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<unknown>("desktop:navigate", (event) => {
      if (event.payload !== "activity" && event.payload !== "settings" && event.payload !== "connections") return;
      setShowSetup(false);
      setNavigation(event.payload === "connections" ? "connection" : event.payload);
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    }).catch(() => {
      // Host navigation is optional. Ordinary in-window navigation remains
      // usable if the native event subscription is unavailable.
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const navigateWithKeyboard = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.repeat) return;
      const page = NAVIGATION[Number(event.key) - 1];
      if (!page) return;
      event.preventDefault();
      setNavigation(page);
    };
    window.addEventListener("keydown", navigateWithKeyboard);
    return () => window.removeEventListener("keydown", navigateWithKeyboard);
  }, []);

  const openSetup = () => {
    setShowSetup(true);
    setNavigation("home");
  };

  const commitState = useCallback((next: DesktopState) => {
    stateVersionRef.current += 1;
    setState(next);
  }, []);

  const commitChatgptActivity = useCallback((next: DesktopState) => {
    stateVersionRef.current += 1;
    setState((current) => {
      if (!current) return next;
      if (current.chatgpt_activity?.observed && !next.chatgpt_activity?.observed) return current;
      return { ...current, chatgpt_activity: next.chatgpt_activity };
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const initial = await desktopApi.getState();
        if (cancelled) return;
        // Keep first-run setup mounted through intermediate topology snapshots
        // and the optional Tunnel handoff, including their error/retry paths.
        if (!initial.topology && !initial.configuration_issue) setShowSetup(true);
        commitState(initial);
        if (initial.current_operation || initial.configuration_issue) return;
        const resumeExisting = Boolean(
          initial.topology
          && initial.runtime_autostart
          && initial.topology.experience === "full",
        );
        // A fresh Desktop must stay in product setup until the user chooses
        // the real project and runtime topology. Silently bootstrapping the
        // Desktop workspace creates a fake "configured" happy path and makes
        // users configure the product twice before ChatGPT can use their code.
        if (!resumeExisting) return;

        setRefreshing(true);
        try {
          const next = await desktopApi.resumeSavedRuntime();
          if (cancelled) return;
          // Backend reconciliation owns every profile's autostart policy.
          commitState(next);
        } catch (value) {
          if (!cancelled) setError(normalizeDesktopError(value));
        } finally {
          if (!cancelled) setRefreshing(false);
        }
      } catch (value) {
        if (!cancelled) setError(normalizeDesktopError(value));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [commitState, startupAttempt]);

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
          try {
            const next = await desktopApi.getState();
            if (!cancelled && stateVersionRef.current === observedVersion) {
              commitState(next);
            }
          } catch {
            // Observation is best-effort. A transient invoke failure must not
            // create an error storm or a second concurrent observer.
          } finally {
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
  }, [commitState, hasCurrentOperation, hasLoadedState, refreshing]);

  useEffect(() => {
    if (!shouldObserveChatgptActivity) return;

    let cancelled = false;
    let timeoutId: number | undefined;
    const observe = async () => {
      try {
        const next = await desktopApi.observeChatgptActivity();
        if (!cancelled) commitChatgptActivity(next);
      } catch {
        // Observation is best-effort. Keep the runtime usable and retry only
        // while the Desktop window remains focused.
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
  }, [commitChatgptActivity, shouldObserveChatgptActivity]);

  useEffect(() => {
    if (navigation === "activity") {
      void desktopApi.activity().then(setActivity).catch(() => undefined);
    }
  }, [navigation, state?.activity_sequence]);

  const runStateOperation = async (operation: () => Promise<DesktopState>) => {
    setError(null);
    try {
      commitState(await operation());
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const chooseLocalProject = async () => {
    if (!state || state.current_operation) return;
    const topology = state.topology;
    if (
      !topology ||
      topology.experience !== "full" ||
      topology.server.kind !== "local" ||
      !state.readiness.runtime_ready
    ) {
      openSetup();
      return;
    }
    setError(null);
    try {
      const selection = await open({
        directory: true,
        multiple: false,
        title: t("setup.chooseProject"),
      });
      if (typeof selection !== "string") return;
      commitState(await desktopApi.activateLocalProject(selection));
      setShowSetup(false);
    } catch (value) {
      setError(normalizeDesktopError(value));
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    try {
      await runStateOperation(desktopApi.refresh);
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
      setError(normalizeDesktopError(value));
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
      setError(normalizeDesktopError(value));
    } finally {
      setCancelSubmittingId((current) => current === observed.id ? null : current);
    }
  };

  return { state, activity, navigation, setNavigation, refreshing, error, setError, cancelSubmittingId, showSetup, setShowSetup, setStartupAttempt, mainRef, commitState, openSetup, chooseLocalProject, refresh, resumeRuntime, cancelCurrentOperation, runStateOperation };
}
