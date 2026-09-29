import { useLocale } from "./locale";
import { PRODUCT_LOCALES } from "./product";
const MESSAGES = {
  title: ["WebCodex managed instructions", "WebCodex 托管指令", "Von WebCodex verwaltete Anweisungen", "Instructions gérées par WebCodex", "WebCodex 管理の指示", "WebCodex 관리 지침"],
  content: ["Global instructions", "全局指令", "Globale Anweisungen", "Instructions globales", "グローバル指示", "전역 지침"],
  save: ["Save instructions", "保存指令", "Anweisungen speichern", "Enregistrer les instructions", "指示を保存", "지침 저장"],
  enable: ["Enable global instructions", "启用全局指令", "Globale Anweisungen aktivieren", "Activer les instructions globales", "グローバル指示を有効化", "전역 지침 사용"],
  configured: ["Included in this Runner configuration", "已加入此 Runner 配置", "In dieser Runner-Konfiguration enthalten", "Inclus dans la configuration de ce Runner", "この Runner の設定に含まれています", "이 Runner 설정에 포함됨"],
  disabled: ["Not enabled for this Runner", "尚未为此 Runner 启用", "Für diesen Runner nicht aktiviert", "Non activé pour ce Runner", "この Runner では未有効", "이 Runner에서 사용 안 함"],
  help: ["Content saves apply to the next instruction read after enabling. No restart is needed. Other instruction files are preserved.", "启用后，内容修改在下一次指令读取时生效，无需重启；已有指令文件保持不变。", "Nach Aktivierung gelten Änderungen beim nächsten Lesen. Kein Neustart; andere Dateien bleiben erhalten.", "Après activation, les modifications prennent effet à la prochaine lecture, sans redémarrage. Les autres fichiers sont conservés.", "有効化後の変更は次の指示読み込みから適用されます。再起動は不要で、他のファイルは保持されます。", "사용 설정 후 다음 지침 읽기부터 변경이 적용됩니다. 재시작은 필요 없으며 다른 파일은 유지됩니다."],
  saveFirst: ["Save your draft before enabling it.", "请先保存草稿，再启用。", "Entwurf vor dem Aktivieren speichern.", "Enregistrez le brouillon avant l’activation.", "有効化する前に下書きを保存してください。", "사용하기 전에 초안을 저장하세요."],
  configure: ["Configure a Runner to enable these instructions. Local drafts can still be saved.", "配置 Runner 后即可启用，当前仍可保存本地指令草稿。", "Zum Aktivieren einen Runner konfigurieren. Lokale Entwürfe können gespeichert werden.", "Configurez un Runner pour activer les instructions. Les brouillons locaux peuvent être enregistrés.", "有効化には Runner を設定してください。ローカル下書きは保存できます。", "사용하려면 Runner를 설정하세요. 로컬 초안은 저장할 수 있습니다."],
  saved: ["File saved. Enabled Runners read it on the next instruction request.", "文件已保存；已启用的 Runner 会在下一次请求指令时读取。", "Datei gespeichert. Aktivierte Runner lesen sie bei der nächsten Anfrage.", "Fichier enregistré. Les Runners activés le liront à la prochaine demande.", "保存しました。有効な Runner は次回の要求で読み込みます。", "저장했습니다. 사용 중인 Runner는 다음 요청에서 읽습니다."],
  applied: ["Enabled without restarting Runner.", "已启用，无需重启 Runner。", "Ohne Runner-Neustart aktiviert.", "Activé sans redémarrer le Runner.", "Runner を再起動せず有効化しました。", "Runner 재시작 없이 사용 설정되었습니다."],
  reload: ["Reload from file", "重新读取文件", "Datei neu laden", "Recharger le fichier", "ファイルを再読込", "파일 다시 읽기"],
  discard: ["Discard draft and reload", "丢弃草稿并重新读取", "Entwurf verwerfen und neu laden", "Abandonner le brouillon et recharger", "下書きを破棄して再読込", "초안 버리고 다시 읽기"],
  keep: ["Keep editing", "继续编辑", "Weiter bearbeiten", "Continuer la modification", "編集を続ける", "계속 편집"],
  unsaved: ["Unsaved changes", "有未保存的修改", "Ungespeicherte Änderungen", "Modifications non enregistrées", "未保存の変更", "저장하지 않은 변경 사항"],
  limit: ["UTF-8 file, at most 1 MiB. The editor loads the complete file, not a preview.", "UTF-8 文件，最大 1 MiB。编辑器读取完整文件，不使用截断预览。", "UTF-8, höchstens 1 MiB. Der Editor lädt die ganze Datei.", "Fichier UTF-8, 1 Mio maximum. L’éditeur charge le fichier complet.", "UTF-8、最大 1 MiB。プレビューではなくファイル全体を読み込みます。", "UTF-8 파일, 최대 1 MiB. 미리보기가 아닌 전체 파일을 읽습니다."],
  tooLarge: ["Draft exceeds 1 MiB and cannot be saved.", "草稿超过 1 MiB，无法保存。", "Entwurf überschreitet 1 MiB.", "Le brouillon dépasse 1 Mio.", "下書きが 1 MiB を超えています。", "초안이 1 MiB를 초과합니다."],
  advanced: ["Additional instruction files (read-only preview)", "其他指令文件（只读预览）", "Weitere Anweisungsdateien (Nur-Lese-Vorschau)", "Autres fichiers d’instructions (aperçu seul)", "追加指示ファイル（読み取り専用）", "추가 지침 파일 (읽기 전용 미리보기)"],
  pathsApplied: ["Instruction and Skill paths applied without restarting Runner.", "指令与 Skill 路径已应用，无需重启 Runner。", "Anweisungs- und Skill-Pfade ohne Neustart angewendet.", "Chemins des instructions et Skills appliqués sans redémarrage.", "指示と Skill のパスを再起動せず適用しました。", "재시작 없이 지침과 Skill 경로를 적용했습니다."],
} as const;
export function useInstructionsText() {
  const { locale } = useLocale();
  const index = PRODUCT_LOCALES.indexOf(locale as typeof PRODUCT_LOCALES[number]);
  return (key: keyof typeof MESSAGES) => MESSAGES[key][index < 0 ? 0 : index];
}
