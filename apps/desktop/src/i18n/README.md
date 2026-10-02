# Desktop interface translations

Each file in `messages/` is a complete language catalog. `locale.tsx` derives the
message-key type from `zh-CN.json` and requires every registered catalog to supply
all keys. `LANGUAGES` is the single list used by both language selectors; labels
use the language's own name.

To add a language, add its JSON catalog, import and register it in `locale.tsx`,
and add its locale code and native name to `LANGUAGES`. Register the locale and
embedded catalog in `src-tauri/src/desktop_locale.rs` and its tests as well.
Append the locale to
`PRODUCT_LOCALES` and append translations to each row in `product.ts`,
`connections-tools.ts`, `runner-capabilities.ts`, `instructions.ts`, and
`service-scope.ts`. Add the corresponding shell translations in `runtime-shell.ts`.
Simplified Chinese (`zh-CN`) and Traditional Chinese (`zh-TW`) use independent
catalogs; `shell-zh-TW.json` covers the legacy shell vocabulary. Preserve `{{name}}`
placeholders and product names. Run `npm test` from `apps/desktop`; tests verify
key parity, nonempty translations, matching placeholders, language switching,
and restoration of every language preference.

The selected locale is stored in WebView local storage and controls document
language and activity time formatting. Unknown or inaccessible stored values
fall back to Simplified Chinese. The UI syncs the locale to the native tray on
mount and on every change. Native menus embed the same `tray.*` catalog entries;
`desktop-locale.json` in the Desktop data directory mirrors the selection for
background startup before the WebView loads. Missing or invalid native preferences
also fall back to Simplified Chinese. Native system dialogs follow OS settings;
raw diagnostic messages remain English. Translations cover the application
interface, not external documentation or backend output.
