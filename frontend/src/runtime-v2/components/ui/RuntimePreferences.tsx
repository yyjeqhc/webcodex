import { ActionIcon, Menu, Tooltip } from "@mantine/core";
import { Languages } from "lucide-react";
import { RUNTIME_LANGUAGES, translate, type RuntimeLanguage } from "../../../runtime_i18n.js";
import type { AppearancePreference } from "../../../runtime_storage.js";
import { AppearanceControls } from "../../../ui/AppearanceControls.js";

type Props = {
  accent: string;
  appearance: AppearancePreference;
  language: RuntimeLanguage;
  onAccentChange: (color: string) => void;
  onAppearanceCycle: () => void;
  onLanguageChange: (language: RuntimeLanguage) => void;
};

/** The same three library-owned controls at the rail, mobile entry and gate. */
export function RuntimePreferences({ accent, appearance, language, onAccentChange, onAppearanceCycle, onLanguageChange }: Props) {
  const t = (value: string) => translate(value, language);
  const languageLabel = RUNTIME_LANGUAGES.find(option => option.value === language)!.label;

  return <div className="runtime-preference-buttons" role="group" aria-label={t("Preferences")}>
    <AppearanceControls accent={accent} appearance={appearance} onAccentChange={onAccentChange}
      onAppearanceCycle={onAppearanceCycle} translate={t} />
    <Menu position="top-end" offset={8} width={184} withinPortal zIndex={1100} shadow="md" radius="md">
      <Menu.Target>
        <Tooltip label={`${t("Language")} · ${languageLabel}`} openDelay={400}>
          <ActionIcon className="runtime-preference-button" variant="subtle" color="gray" size={36} radius="md" aria-label={t("Language")}>
            <Languages size={18} />
          </ActionIcon>
        </Tooltip>
      </Menu.Target>
      <Menu.Dropdown className="appearance-preference-popover">
        <Menu.Label>{t("Language")}</Menu.Label>
        <Menu.RadioGroup value={language} onChange={value => onLanguageChange(value as RuntimeLanguage)}>
          {RUNTIME_LANGUAGES.map(option => <Menu.RadioItem key={option.value} value={option.value} closeMenuOnClick>
            {option.label}
          </Menu.RadioItem>)}
        </Menu.RadioGroup>
      </Menu.Dropdown>
    </Menu>
  </div>;
}
