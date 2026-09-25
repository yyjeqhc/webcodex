import { Alert, Button, Modal, PasswordInput, Select, TextInput, Textarea, MantineProvider, colorsTuple, createTheme } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { MotionConfig } from "motion/react";
import type { ReactNode } from "react";
import { accentTokens } from "../../../../frontend/src/ui/accent";
import { accentVariantResolver } from "../../../../frontend/src/ui/mantineAccentTheme";
import { useAppearance } from "../hooks/useAppearance";

export function DesktopMantineProvider({ children }: { children: ReactNode }) {
  const { accent, appearance } = useAppearance();
  const prefersDark = useMediaQuery("(prefers-color-scheme: dark)", false);
  const colorScheme = appearance === "system"
    ? prefersDark ? "dark" : "light"
    : appearance;

  const theme = createTheme({
    primaryColor: "brand",
    primaryShade: { light: 6, dark: 6 },
    colors: { brand: colorsTuple(accentTokens(accent, colorScheme).accent) },
    variantColorResolver: accentVariantResolver(accent, colorScheme),
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", ui-sans-serif, sans-serif',
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
      Modal: Modal.extend({ defaultProps: { centered: true, overlayProps: { backgroundOpacity: 0.35, blur: 5 } }, classNames: { content: "ui-mantine-modal-content", header: "ui-mantine-modal-header", title: "ui-mantine-modal-title" } }),
      Alert: Alert.extend({ classNames: { root: "ui-mantine-alert" } }),
    },
  });
  return (
    <MantineProvider
      forceColorScheme={colorScheme}
      theme={theme}
    >
      <MotionConfig reducedMotion="user">{children}</MotionConfig>
    </MantineProvider>
  );
}
