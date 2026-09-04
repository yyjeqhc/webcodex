import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  LOCALE_STORAGE_KEY,
  LocaleProvider,
  useLocale,
} from "./locale";

function LocaleProbe() {
  const { locale, setLocale, t } = useLocale();
  return (
    <label>
      {t("locale.label")}
      <select
        aria-label={t("locale.label")}
        value={locale}
        onChange={(event) => setLocale(event.target.value as typeof locale)}
      >
        <option value="zh-CN">简体中文</option>
        <option value="en-US">English</option>
      </select>
    </label>
  );
}

describe("LocaleProvider", () => {
  it("defaults to zh-CN, persists English, and restores the preference after remount", async () => {
    const first = render(
      <LocaleProvider>
        <LocaleProbe />
      </LocaleProvider>,
    );

    const chineseSelect = screen.getByRole("combobox", { name: "界面语言" });
    expect(chineseSelect).toHaveValue("zh-CN");
    await waitFor(() => expect(document.documentElement).toHaveAttribute("lang", "zh-CN"));

    fireEvent.change(chineseSelect, { target: { value: "en-US" } });
    await waitFor(() => expect(document.documentElement).toHaveAttribute("lang", "en-US"));
    expect(window.localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("en-US");

    first.unmount();
    render(
      <LocaleProvider>
        <LocaleProbe />
      </LocaleProvider>,
    );
    const englishSelect = screen.getByRole("combobox", { name: "Language" });
    expect(englishSelect).toHaveValue("en-US");

    fireEvent.change(englishSelect, { target: { value: "zh-CN" } });
    await waitFor(() => expect(document.documentElement).toHaveAttribute("lang", "zh-CN"));
    expect(window.localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("zh-CN");
  });
});
