import { useLocale } from "./locale";
import { PRODUCT_LOCALES } from "./product";
const MESSAGES = {
  title: ["WebCodex managed instructions", "WebCodex 托管指令", "Von WebCodex verwaltete Anweisungen", "Instructions gérées par WebCodex", "WebCodex 管理の指示", "WebCodex 관리 지침", "WebCodex 託管指令"],
  content: ["Global instructions", "全局指令", "Globale Anweisungen", "Instructions globales", "グローバル指示", "전역 지침", "全域指令"],
  save: ["Save instructions", "保存指令", "Anweisungen speichern", "Enregistrer les instructions", "指示を保存", "지침 저장", "儲存指令"],
  enable: ["Enable global instructions", "启用全局指令", "Globale Anweisungen aktivieren", "Activer les instructions globales", "グローバル指示を有効化", "전역 지침 사용", "啟用全域指令"],
  configured: ["Included in this Runner configuration", "已加入此 Runner 配置", "In dieser Runner-Konfiguration enthalten", "Inclus dans la configuration de ce Runner", "この Runner の設定に含まれています", "이 Runner 설정에 포함됨", "已加入此 Runner 設定"],
  disabled: ["Not enabled for this Runner", "尚未为此 Runner 启用", "Für diesen Runner nicht aktiviert", "Non activé pour ce Runner", "この Runner では未有効", "이 Runner에서 사용 안 함", "尚未為此 Runner 啟用"],
  help: ["Set guidance for AI across your projects. Changes apply on the next instruction read.", "为 AI 设置各项目通用的工作指令；修改在下一次读取时生效。", "Projektübergreifende Anweisungen für KI. Änderungen gelten beim nächsten Lesen.", "Définissez des instructions communes à vos projets. Les modifications s’appliquent à la prochaine lecture.", "プロジェクト共通の AI 指示を設定します。変更は次の読み込みから適用されます。", "프로젝트 공통 AI 지침을 설정하세요. 변경은 다음 읽기부터 적용됩니다.", "為 AI 設定各專案通用的工作指令；修改在下一次讀取時生效。"],
  saveFirst: ["Save your draft before enabling it.", "请先保存草稿，再启用。", "Entwurf vor dem Aktivieren speichern.", "Enregistrez le brouillon avant l’activation.", "有効化する前に下書きを保存してください。", "사용하기 전에 초안을 저장하세요.", "請先儲存草稿，再啟用。"],
  configure: ["Configure a Runner to enable these instructions. Local drafts can still be saved.", "配置 Runner 后即可启用，当前仍可保存本地指令草稿。", "Zum Aktivieren einen Runner konfigurieren. Lokale Entwürfe können gespeichert werden.", "Configurez un Runner pour activer les instructions. Les brouillons locaux peuvent être enregistrés.", "有効化には Runner を設定してください。ローカル下書きは保存できます。", "사용하려면 Runner를 설정하세요. 로컬 초안은 저장할 수 있습니다.", "設定 Runner 後即可啟用，當前仍可儲存本機指令草稿。"],
  saved: ["File saved. Enabled Runners read it on the next instruction request.", "文件已保存；已启用的 Runner 会在下一次请求指令时读取。", "Datei gespeichert. Aktivierte Runner lesen sie bei der nächsten Anfrage.", "Fichier enregistré. Les Runners activés le liront à la prochaine demande.", "保存しました。有効な Runner は次回の要求で読み込みます。", "저장했습니다. 사용 중인 Runner는 다음 요청에서 읽습니다.", "檔案已儲存；已啟用的 Runner 會在下一次請求指令時讀取。"],
  applied: ["Enabled without restarting Runner.", "已启用，无需重启 Runner。", "Ohne Runner-Neustart aktiviert.", "Activé sans redémarrer le Runner.", "Runner を再起動せず有効化しました。", "Runner 재시작 없이 사용 설정되었습니다.", "已啟用，無需重啟 Runner。"],
  reload: ["Reload from file", "重新读取文件", "Datei neu laden", "Recharger le fichier", "ファイルを再読込", "파일 다시 읽기", "重新讀取檔案"],
  discard: ["Discard draft and reload", "丢弃草稿并重新读取", "Entwurf verwerfen und neu laden", "Abandonner le brouillon et recharger", "下書きを破棄して再読込", "초안 버리고 다시 읽기", "丟棄草稿並重新讀取"],
  keep: ["Keep editing", "继续编辑", "Weiter bearbeiten", "Continuer la modification", "編集を続ける", "계속 편집", "繼續編輯"],
  unsaved: ["Unsaved changes", "有未保存的修改", "Ungespeicherte Änderungen", "Modifications non enregistrées", "未保存の変更", "저장하지 않은 변경 사항", "有未儲存的修改"],
  limit: ["Up to 1 MiB", "最多 1 MiB", "Bis zu 1 MiB", "1 Mio maximum", "最大 1 MiB", "최대 1 MiB", "最多 1 MiB"],
  tooLarge: ["Draft exceeds 1 MiB and cannot be saved.", "草稿超过 1 MiB，无法保存。", "Entwurf überschreitet 1 MiB.", "Le brouillon dépasse 1 Mio.", "下書きが 1 MiB を超えています。", "초안이 1 MiB를 초과합니다.", "草稿超過 1 MiB，無法儲存。"],
  advanced: ["Additional instruction files (read-only preview)", "其他指令文件（只读预览）", "Weitere Anweisungsdateien (Nur-Lese-Vorschau)", "Autres fichiers d’instructions (aperçu seul)", "追加指示ファイル（読み取り専用）", "추가 지침 파일 (읽기 전용 미리보기)", "其他指令檔案（只讀預覽）"],
  pathsApplied: ["Instruction and Skill paths applied without restarting Runner.", "指令与 Skill 路径已应用，无需重启 Runner。", "Anweisungs- und Skill-Pfade ohne Neustart angewendet.", "Chemins des instructions et Skills appliqués sans redémarrage.", "指示と Skill のパスを再起動せず適用しました。", "재시작 없이 지침과 Skill 경로를 적용했습니다.", "指令與 Skill 路徑已應用，無需重啟 Runner。"],
} as const;
export function useInstructionsText() {
  const { locale } = useLocale();
  const index = PRODUCT_LOCALES.indexOf(locale as typeof PRODUCT_LOCALES[number]);
  return (key: keyof typeof MESSAGES) => MESSAGES[key][index < 0 ? 0 : index];
}
