import { ActionIcon, ColorSwatch, Group, Popover, Stack, Text, TextInput, Tooltip } from "@mantine/core";
import { Check, Monitor, MoonStar, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { ACCENT_PRESETS, normalizeAccent } from "./accent.js";

type Props = {
  accent: string;
  appearance: "system" | "light" | "dark";
  onAccentChange: (color: string) => void;
  onAppearanceCycle: () => void;
  translate?: (value: string) => string;
};

/** Shared Web controls; each renderer owns its preference state and layout. */
export function AppearanceControls({ accent, appearance, onAccentChange, onAppearanceCycle, translate: t = value => value }: Props) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [customColor, setCustomColor] = useState(accent);
  useEffect(() => setCustomColor(accent), [accent]);
  const appearanceLabel = `${t("Appearance")} · ${t(appearance === "system" ? "System" : appearance === "light" ? "Light" : "Dark")}`;
  return <>
    <Popover opened={paletteOpen} onChange={setPaletteOpen} width={244} position="top-start" offset={8}
      withinPortal trapFocus returnFocus zIndex={1100} shadow="md" radius="md">
      <Popover.Target>
        <Tooltip label={t("Accent color")} openDelay={400} disabled={paletteOpen}>
          <ActionIcon className="appearance-preference-button" variant="subtle" color="gray" size={36} radius="md"
            aria-label={t("Accent color")} onClick={() => setPaletteOpen(value => !value)}>
            <ColorSwatch color={accent} size={16} withShadow={false} aria-hidden="true" />
          </ActionIcon>
        </Tooltip>
      </Popover.Target>
      <Popover.Dropdown className="appearance-preference-popover" aria-label={t("Accent color")}>
        <Stack gap="md">
          <Text size="sm" fw={600}>{t("Accent color")}</Text>
          <Group gap="sm" justify="space-between">
            {ACCENT_PRESETS.map(preset => <Tooltip key={preset.id} label={t(preset.label)}>
              <ColorSwatch component="button" type="button" color={preset.color} size={32}
                aria-label={t(preset.label)} aria-pressed={accent.toLowerCase() === preset.color}
                data-autofocus={accent.toLowerCase() === preset.color || undefined}
                onClick={() => { onAccentChange(preset.color); setPaletteOpen(false); }}>
                {accent.toLowerCase() === preset.color && <Check size={17} color="white" aria-hidden="true" />}
              </ColorSwatch>
            </Tooltip>)}
          </Group>
          <TextInput size="xs" label={t("Custom color")} value={customColor} maxLength={7} placeholder="#2563eb"
            onChange={event => {
              const value = event.currentTarget.value;
              setCustomColor(value);
              const normalized = normalizeAccent(value);
              if (normalized) onAccentChange(normalized);
            }} onBlur={() => setCustomColor(accent)} />
        </Stack>
      </Popover.Dropdown>
    </Popover>
    <Tooltip label={appearanceLabel} openDelay={400}>
      <ActionIcon className="appearance-preference-button" variant="subtle" color="gray" size={36} radius="md"
        aria-label={t("Appearance")} aria-description={appearanceLabel} onClick={onAppearanceCycle}>
        {appearance === "system" ? <Monitor size={18} /> : appearance === "dark" ? <MoonStar size={18} /> : <Sun size={18} />}
      </ActionIcon>
    </Tooltip>
  </>;
}
