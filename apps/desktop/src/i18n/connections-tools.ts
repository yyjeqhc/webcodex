import { useLocale } from "./locale";
import { PRODUCT_LOCALES } from "./product";

// English, Chinese, German, French, Japanese, Korean; names remain user text.
export const CONNECTIONS_TOOLS_MESSAGES = {
  connections: ["Connections", "连接", "Verbindungen", "Connexions", "接続", "연결"],
  addConnection: ["Add Connection", "添加连接", "Verbindung hinzufügen", "Ajouter une connexion", "接続を追加", "연결 추가"],
  name: ["Name", "名称", "Name", "Nom", "名前", "이름"],
  autostart: ["Start automatically", "自动启动", "Automatisch starten", "Démarrer automatiquement", "自動的に起動", "자동으로 시작"],
  secureTunnel: ["Secure MCP Tunnel", "安全 MCP Tunnel", "Sicherer MCP-Tunnel", "Tunnel MCP sécurisé", "セキュア MCP トンネル", "보안 MCP 터널"],
  saveApply: ["Save & Apply", "保存并应用", "Speichern und anwenden", "Enregistrer et appliquer", "保存して適用", "저장 및 적용"],
  credentials: ["Credentials present", "已配置凭据", "Zugangsdaten vorhanden", "Identifiants configurés", "認証情報を設定済み", "자격 증명 설정됨"],
  copyId: ["Copy ID", "复制 ID", "ID kopieren", "Copier l’ID", "ID をコピー", "ID 복사"],
  noConnections: ["No connections yet", "尚未添加连接", "Noch keine Verbindungen", "Aucune connexion", "接続はまだありません", "아직 연결 없음"],
  sharedRuntime: ["Connections share one Server, Runner and workspace.", "连接共享同一个 Server、Runner 和工作区。", "Verbindungen teilen einen Server, Runner und Arbeitsbereich.", "Les connexions partagent un serveur, un Runner et un espace de travail.", "接続は同じ Server、Runner、ワークスペースを共有します。", "연결은 하나의 Server, Runner 및 작업 공간을 공유합니다."],
  localRuntimeNeeded: ["Start the local runtime to connect. Your settings remain saved.", "启动本地 Runtime 后即可连接，配置已保留。", "Lokale Laufzeit zum Verbinden starten. Einstellungen bleiben gespeichert.", "Démarrez l’environnement local. Vos paramètres restent enregistrés.", "接続するにはローカルランタイムを起動してください。設定は保存済みです。", "연결하려면 로컬 런타임을 시작하세요. 설정은 저장됩니다."],
  delete: ["Delete", "删除", "Löschen", "Supprimer", "削除", "삭제"],
  remove: ["Remove", "移除", "Entfernen", "Retirer", "削除", "제거"],
  deleteConnectionHelp: ["Stop this connection and delete its saved credential. Other connections are unchanged.", "停止此连接并删除其凭据，不影响其他连接。", "Diese Verbindung beenden und Zugangsdaten löschen. Andere Verbindungen bleiben unverändert.", "Arrêter cette connexion et supprimer ses identifiants. Les autres restent inchangées.", "この接続を停止し認証情報を削除します。他の接続には影響しません。", "이 연결을 중지하고 자격 증명을 삭제합니다. 다른 연결에는 영향이 없습니다."],
  operationFailed: ["Could not apply the change. Refresh and check the saved settings before retrying.", "未能应用更改。请刷新并检查已保存的配置后重试。", "Änderung fehlgeschlagen. Aktualisieren und gespeicherte Einstellungen prüfen.", "Modification impossible. Actualisez et vérifiez les paramètres enregistrés.", "変更を適用できませんでした。更新して保存された設定を確認してください。", "변경을 적용하지 못했습니다. 새로 고침 후 저장된 설정을 확인하세요."],
  configError: ["Saved configuration is unavailable. The original file has not been replaced.", "无法读取配置，原始文件未被覆盖。", "Konfiguration nicht verfügbar. Originaldatei wurde nicht ersetzt.", "Configuration indisponible. Le fichier d’origine n’a pas été remplacé.", "設定を読み込めません。元のファイルは変更していません。", "설정을 읽을 수 없습니다. 원본 파일은 교체되지 않았습니다."],
  localUnavailable: ["Waiting for the local MCP endpoint", "等待本地 MCP 端点恢复", "Warten auf lokalen MCP-Endpunkt", "En attente du point MCP local", "ローカル MCP エンドポイントを待機中", "로컬 MCP 엔드포인트 대기 중"],
  tunnelUnavailable: ["Check this connection’s credentials and network, then restart it.", "请检查此连接的凭据和网络，然后重新启动。", "Zugangsdaten und Netzwerk prüfen, dann Verbindung neu starten.", "Vérifiez les identifiants et le réseau, puis redémarrez cette connexion.", "この接続の認証情報とネットワークを確認し、再起動してください。", "이 연결의 자격 증명과 네트워크를 확인한 후 다시 시작하세요."],
  tunnelDownloadFailed: ["The verified tunnel client could not be downloaded. Check network and proxy settings, then retry.", "无法下载已验证的隧道客户端。请检查网络和代理设置后重试。", "Der verifizierte Tunnel-Client konnte nicht heruntergeladen werden. Netzwerk und Proxy-Einstellungen prüfen und erneut versuchen.", "Le client de tunnel vérifié n’a pas pu être téléchargé. Vérifiez le réseau et le proxy, puis réessayez.", "検証済みトンネルクライアントをダウンロードできませんでした。ネットワークとプロキシ設定を確認して再試行してください。", "검증된 터널 클라이언트를 다운로드하지 못했습니다. 네트워크와 프록시 설정을 확인한 후 다시 시도하세요."],
  tunnelInstallFailed: ["The tunnel client could not be installed. Check disk space and app data permissions, then retry.", "无法安装隧道客户端。请检查磁盘空间和应用数据目录权限后重试。", "Der Tunnel-Client konnte nicht installiert werden. Speicherplatz und Zugriffsrechte der App-Daten prüfen und erneut versuchen.", "Le client de tunnel n’a pas pu être installé. Vérifiez l’espace disque et les autorisations des données de l’application, puis réessayez.", "トンネルクライアントをインストールできませんでした。空き容量とアプリデータの権限を確認して再試行してください。", "터널 클라이언트를 설치하지 못했습니다. 디스크 공간과 앱 데이터 권한을 확인한 후 다시 시도하세요."],
  tunnelVerificationFailed: ["The tunnel client failed integrity verification. Retry the verified download; do not bypass verification.", "隧道客户端完整性校验失败。请重试下载，不要绕过校验。", "Die Integritätsprüfung des Tunnel-Clients ist fehlgeschlagen. Verifizierten Download erneut versuchen; Prüfung nicht umgehen.", "La vérification d’intégrité du client de tunnel a échoué. Relancez le téléchargement vérifié sans contourner la vérification.", "トンネルクライアントの整合性検証に失敗しました。検証を回避せず、ダウンロードを再試行してください。", "터널 클라이언트 무결성 검증에 실패했습니다. 검증을 우회하지 말고 다운로드를 다시 시도하세요."],
  stopping: ["Stopping", "停止中", "Wird beendet", "Arrêt en cours", "停止処理中", "중지 중"],
  logs: ["Connection events", "连接事件", "Verbindungsereignisse", "Événements de connexion", "接続イベント", "연결 이벤트"],
  mcpProviders: ["MCP Providers", "MCP Providers", "MCP-Anbieter", "Fournisseurs MCP", "MCP プロバイダー", "MCP 공급자"],
  addMcpProvider: ["Add MCP Provider", "添加 MCP Provider", "MCP-Anbieter hinzufügen", "Ajouter un fournisseur MCP", "MCP プロバイダーを追加", "MCP 공급자 추가"],
  providerName: ["Provider Name", "Provider 名称", "Anbietername", "Nom du fournisseur", "プロバイダー名", "공급자 이름"],
  command: ["Command", "命令", "Befehl", "Commande", "コマンド", "명령"],
  arguments: ["Arguments", "参数", "Argumente", "Arguments", "引数", "인수"],
  argsHelp: ["JSON array, e.g. [\"-y\", \"@playwright/mcp\"]. Use private environment fields for credentials.", "JSON 数组，例如 [\"-y\", \"@playwright/mcp\"]。凭据请放入私密环境变量。", "JSON-Array, z. B. [\"-y\", \"@playwright/mcp\"]. Zugangsdaten in private Umgebungsfelder eintragen.", "Tableau JSON, ex. [\"-y\", \"@playwright/mcp\"]. Utilisez l’environnement privé pour les identifiants.", "JSON 配列（例：[\"-y\", \"@playwright/mcp\"]）。認証情報は非公開の環境変数に入力してください。", "JSON 배열 예: [\"-y\", \"@playwright/mcp\"]. 자격 증명은 비공개 환경 변수에 입력하세요."],
  invalidFields: ["Check the arguments array and unique environment names.", "请检查参数数组，环境变量名称不可重复。", "Argument-Array und eindeutige Umgebungsnamen prüfen.", "Vérifiez le tableau d’arguments et les noms de variables uniques.", "引数の配列と重複のない環境変数名を確認してください。", "인수 배열과 고유한 환경 변수 이름을 확인하세요."],
  workingDirectory: ["Working directory", "工作目录", "Arbeitsverzeichnis", "Répertoire de travail", "作業ディレクトリ", "작업 디렉터리"],
  environment: ["Private environment variables", "私密环境变量", "Private Umgebungsvariablen", "Variables d’environnement privées", "非公開の環境変数", "비공개 환경 변수"],
  addEnvironment: ["Add Environment Variable", "添加环境变量", "Umgebungsvariable hinzufügen", "Ajouter une variable", "環境変数を追加", "환경 변수 추가"],
  envName: ["Variable name", "变量名", "Variablenname", "Nom de variable", "変数名", "변수 이름"],
  envValue: ["Private value", "私密值", "Privater Wert", "Valeur privée", "非公開の値", "비공개 값"],
  keepCredential: ["Leave blank to retain the saved value", "留空以保留已保存的值", "Leer lassen, um den Wert zu behalten", "Laisser vide pour conserver la valeur", "保存した値を保持するには空欄にしてください", "저장된 값을 유지하려면 비워 두세요"],
  enabled: ["Enabled", "已启用", "Aktiviert", "Activé", "有効", "사용"],
  disabled: ["Disabled", "已禁用", "Deaktiviert", "Désactivé", "無効", "사용 안 함"],
  configured: ["Configured", "已配置", "Konfiguriert", "Configuré", "設定済み", "설정됨"],
  saved: ["Saved", "已保存", "Gespeichert", "Enregistré", "保存済み", "저장됨"],
  noProviders: ["No MCP Providers configured", "尚未配置 MCP Provider", "Keine MCP-Anbieter konfiguriert", "Aucun fournisseur MCP configuré", "MCP プロバイダーは未設定です", "설정된 MCP 공급자 없음"],
  sharedProviders: ["Runner-scoped tools shared by all projects and connections. Restart Runner after saving to apply.", "Runner 级工具，由所有项目和连接共享。保存后重启 Runner 生效。", "Runner-weite Werkzeuge für alle Projekte und Verbindungen. Nach dem Speichern Runner neu starten.", "Outils de Runner partagés par les projets et connexions. Redémarrez Runner après l’enregistrement.", "すべてのプロジェクトと接続で共有するツールです。保存後に Runner を再起動してください。", "모든 프로젝트와 연결이 공유하는 Runner 범위 도구입니다. 저장 후 Runner를 다시 시작하세요."],
  deleteProviderHelp: ["Delete this provider and its private values. Restart Runner to remove it from running tools.", "删除此 Provider 及其私密配置，重启 Runner 后从运行中的工具中移除。", "Anbieter und private Werte löschen. Runner neu starten, um laufende Werkzeuge zu aktualisieren.", "Supprimer ce fournisseur et ses valeurs privées. Redémarrez Runner pour actualiser les outils actifs.", "プロバイダーと非公開の値を削除します。実行中のツールに反映するには Runner を再起動してください。", "공급자와 비공개 값을 삭제합니다. 실행 중인 도구에 반영하려면 Runner를 다시 시작하세요."],
  nativePlugins: ["Advanced: Native Tool Plugins", "高级：Native Tool Plugins", "Erweitert: Native Tool-Plugins", "Avancé : plugins natifs", "詳細：ネイティブツールプラグイン", "고급: 네이티브 도구 플러그인"],
  capacity: ["Enabled providers", "已启用的 Provider", "Aktivierte Anbieter", "Fournisseurs activés", "有効なプロバイダー", "활성 공급자"],
} as const;
export type ConnectionsToolsKey = keyof typeof CONNECTIONS_TOOLS_MESSAGES;
export function connectionsToolsText(locale: string, key: ConnectionsToolsKey): string {
  const index = PRODUCT_LOCALES.indexOf(locale as typeof PRODUCT_LOCALES[number]);
  return CONNECTIONS_TOOLS_MESSAGES[key][index < 0 ? 0 : index];
}
export function useConnectionsTools() {
  const { locale } = useLocale();
  return (key: ConnectionsToolsKey) => connectionsToolsText(locale, key);
}
