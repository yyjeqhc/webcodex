import { AccentPalette } from "../../../../frontend/src/ui/AccentPalette";
import { useLocale } from "../i18n/locale";

export function AccentPicker({ color, onChange, compact = false }: { color: string; onChange: (color: string) => void; compact?: boolean }) {
  const { t } = useLocale();
  return <AccentPalette color={color} onChange={onChange} compact={compact} label={t("accent.label")} customLabel={t("accent.custom")} />;
}
