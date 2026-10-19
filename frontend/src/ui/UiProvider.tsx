import { Alert, Button, Menu, Modal, PasswordInput, SegmentedControl, Select, TextInput, Textarea, MantineProvider, colorsTuple, createTheme } from "@mantine/core";
import { MotionConfig } from "motion/react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ACCENT_CHANGE_EVENT, ACCENT_STORAGE_KEY, accentTokens, loadAccentPreference, normalizeAccent } from "./accent.js";
import { accentVariantResolver } from "./mantineAccentTheme.js";

function resolvedTheme(): "light" | "dark" {
  return document.documentElement.dataset.resolvedTheme === "dark" ? "dark" : "light";
}

/** Keeps Mantine's controls in step with the existing WebCodex appearance preference. */
export function UiProvider({ children }: { children: ReactNode }) {
  const [colorScheme, setColorScheme] = useState(resolvedTheme);
  const [accent, setAccent] = useState(loadAccentPreference);

  useEffect(() => {
    const observer = new MutationObserver(() => setColorScheme(resolvedTheme()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-resolved-theme"] });
    const updateAccent = (event: Event) => {
      const next = event instanceof StorageEvent
        ? event.key === ACCENT_STORAGE_KEY ? normalizeAccent(event.newValue) : null
        : normalizeAccent((event as CustomEvent<string>).detail);
      if (next) setAccent(next);
    };
    window.addEventListener(ACCENT_CHANGE_EVENT, updateAccent);
    window.addEventListener("storage", updateAccent);
    return () => {
      observer.disconnect();
      window.removeEventListener(ACCENT_CHANGE_EVENT, updateAccent);
      window.removeEventListener("storage", updateAccent);
    };
  }, []);

  const theme = useMemo(() => createTheme({
    primaryColor: "brand",
    primaryShade: { light: 6, dark: 6 },
    colors: { brand: colorsTuple(accentTokens(accent, colorScheme).accent) },
    variantColorResolver: accentVariantResolver(accent, colorScheme),
    fontFamily: "var(--ui-font-family)",
    fontSizes: { xs: "12px", sm: "13px", md: "14px", lg: "16px", xl: "20px" },
    spacing: { xs: "4px", sm: "8px", md: "12px", lg: "16px", xl: "24px" },
    radius: { xs: "4px", sm: "8px", md: "12px", lg: "16px", xl: "20px" },
    defaultRadius: "md",
    components: {
      Button: Button.extend({ defaultProps: { size: "sm" }, classNames: { root: "ui-mantine-button" } }),
      TextInput: TextInput.extend({ defaultProps: { size: "sm" }, classNames: { input: "ui-mantine-input", label: "ui-mantine-label" } }),
      PasswordInput: PasswordInput.extend({ defaultProps: { size: "sm" }, classNames: { input: "ui-mantine-input", label: "ui-mantine-label" } }),
      Select: Select.extend({ defaultProps: { size: "sm" }, classNames: { input: "ui-mantine-input", label: "ui-mantine-label" } }),
      Textarea: Textarea.extend({ defaultProps: { size: "sm" }, classNames: { input: "ui-mantine-input", label: "ui-mantine-label" } }),
      Modal: Modal.extend({ classNames: { content: "ui-mantine-modal-content", header: "ui-mantine-modal-header", title: "ui-mantine-modal-title" } }),
      Alert: Alert.extend({ classNames: { root: "ui-mantine-alert" } }),
      SegmentedControl: SegmentedControl.extend({ classNames: { root: "ui-mantine-segments", indicator: "ui-mantine-segment-indicator" } }),
      Menu: Menu.extend({ classNames: { dropdown: "ui-mantine-menu-dropdown" } }),
    },
  }), [accent, colorScheme]);

  return <MantineProvider theme={theme} forceColorScheme={colorScheme}>
    <MotionConfig reducedMotion="user">{children}</MotionConfig>
  </MantineProvider>;
}
