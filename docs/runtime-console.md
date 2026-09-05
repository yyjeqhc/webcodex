# Runtime Console navigation

Open `/runtime` and connect with an existing runtime credential.

- **Projects & Sessions** is the collaboration workspace. Select a Project and
  Workflow Session in the sidebar. Recent Sessions starts expanded and can be
  collapsed when more room is needed.
- **Context** opens the selected Session's context. **Overview** shows work,
  attention, validation, and model-reported progress directly. **Activity** shows
  retained events and the existing follow-latest control. **Details** shows
  identity, lifecycle, mode, timestamps, and workspace information.
- **Runtime & Agents** provides three separate destinations: **Overview**,
  **Runner fleet**, and **Durable Agents**. Selecting a destination shows its
  full content and updates the navigation highlight. Switching destinations
  keeps existing forms mounted so unsent input is retained.

The context panel still adapts between a docked rail, popover, and mobile sheet.
Closing it leaves a labeled Context entry in the header. Context navigation uses
ordinary keyboard-focusable buttons, with the current choice announced as pressed.
Mobile operation navigation closes after selection and focuses the destination.

These are presentation changes. Workflow Session and durable Agent identities,
credential scopes, refresh behavior, and mutation handling keep their existing
contracts. Model-reported progress remains informational.
