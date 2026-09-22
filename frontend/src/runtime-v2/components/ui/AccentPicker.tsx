import { AccentPalette } from "../../../ui/AccentPalette.js";
import type { RuntimeLanguage } from "../../../runtime_i18n.js";
import { translate } from "../../../runtime_i18n.js";

export function AccentPicker({ color, onChange, language }: { color: string; onChange: (color: string) => void; language: RuntimeLanguage }) {
  const t = (value: string) => translate(value, language);
  return <AccentPalette color={color} onChange={onChange} label={t("Accent color")} customLabel={t("Custom color")} colorLabel={t} />;
}
