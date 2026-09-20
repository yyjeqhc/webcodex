import { useEffect, useState } from "react";
import { fetchProjectGit } from "../api/projects.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { ProjectGit } from "../model/types.js";

export function useProjectGit(client: RuntimeV2Client, enabled: boolean, projectId: string): ProjectGit | null {
  const [git, setGit] = useState<ProjectGit | null>(null);
  useEffect(() => {
    if (!enabled || !projectId) {
      setGit(null);
      return;
    }
    const controller = new AbortController();
    void fetchProjectGit(client, projectId, controller.signal).then((response) => {
      if (!controller.signal.aborted) setGit(response?.ok && response.data ? response.data : null);
    });
    return () => controller.abort();
  }, [client, enabled, projectId]);
  return git;
}
