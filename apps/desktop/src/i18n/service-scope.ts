import { useLocale } from "./locale";
import { PRODUCT_LOCALES } from "./product";
const TEXT = {
  title: ["Background startup", "后台启动方式", "Hintergrundstart", "Démarrage en arrière-plan", "バックグラウンド起動", "백그라운드 시작", "背景啟動方式"],
  user: ["When I sign in (recommended)", "我登录后启动（推荐）", "Bei meiner Anmeldung (empfohlen)", "À ma connexion (recommandé)", "ログイン時（推奨）", "로그인할 때 (권장)", "我登入後啟動（推薦）"],
  system: ["At computer startup (advanced)", "计算机启动时（高级）", "Beim Systemstart (erweitert)", "Au démarrage de l’ordinateur (avancé)", "コンピュータ起動時（高度な設定）", "컴퓨터 시작 시 (고급)", "電腦啟動時（高階）"],
  userHelp: ["Uses your own service manager, with no service password. macOS and Windows require a signed-in session; Linux logout persistence depends on linger. No system-service fallback.", "使用你的用户级服务，无需服务账户密码。macOS、Windows 需要保持登录；Linux 退出后继续运行取决于 linger。不会自动改装成系统服务。", "Eigene Benutzerverwaltung, ohne Dienstpasswort. macOS/Windows benötigen eine angemeldete Sitzung; Linux hängt von linger ab. Kein Systemdienst-Fallback.", "Services de votre session, sans mot de passe de service. Session connectée requise sur macOS/Windows ; linger détermine la persistance sous Linux. Aucun basculement système.", "サービス用パスワード不要のユーザー管理を使用します。macOS/Windows はログイン中のみ、Linux は linger に依存します。システムサービスへの自動切替はありません。", "서비스 암호 없이 사용자 서비스를 사용합니다. macOS/Windows는 로그인 세션이 필요하며 Linux는 linger 설정에 따릅니다. 시스템 서비스로 자동 전환하지 않습니다.", "使用你的使用者級服務，無需服務賬戶密碼。macOS、Windows 需要保持登入；Linux 退出後繼續執行取決於 linger。不會自動改裝成系統服務。"],
  systemHelp: ["Uses machine services and requests OS authorization. Windows Runner setup may require the selected account’s password. Existing services are never adopted automatically.", "使用系统级服务并请求操作系统授权；Windows Runner 可能需要所选账户密码。不会自动接管已有服务。", "Verwendet Systemdienste mit Betriebssystemautorisierung. Windows-Runner kann das Kontopasswort benötigen. Bestehende Dienste werden nicht automatisch übernommen.", "Services système avec autorisation du système. Le Runner Windows peut nécessiter le mot de passe du compte. Aucun service existant n’est adopté automatiquement.", "OS の認可を得てシステムサービスを使用します。Windows Runner ではアカウントパスワードが必要な場合があります。既存サービスは自動取得しません。", "OS 승인을 받아 시스템 서비스를 사용합니다. Windows Runner는 계정 암호가 필요할 수 있습니다. 기존 서비스를 자동 인수하지 않습니다.", "使用系統級服務並請求作業系統授權；Windows Runner 可能需要所選賬戶密碼。不會自動接管已有服務。"],
} as const;
export function useServiceScopeText() {
  const { locale } = useLocale();
  const index = PRODUCT_LOCALES.indexOf(locale as typeof PRODUCT_LOCALES[number]);
  return (key: keyof typeof TEXT) => TEXT[key][index < 0 ? 0 : index];
}
