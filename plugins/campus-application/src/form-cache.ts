import { createHash } from "node:crypto";
import {
  resumePathForCanonicalField,
  type CanonicalField,
} from "./resume.js";

export type SnapshotFormContext = {
  field_signature: string;
  dom_tag: string;
  input_type?: string;
  html_name?: string;
  placeholder?: string;
  autocomplete?: string;
  nearby_label?: string;
  group_label?: string;
  group_index?: number;
  group_size?: number;
  section_label?: string;
  component_hint?: string;
  aria_invalid?: boolean;
  validation_hint?: string;
  option_count?: number;
};

export type SnapshotNode = {
  role: string;
  name: string;
  description?: string;
  value?: string;
  group_id?: string;
  group_role?: string;
  group_label?: string;
  checked?: string;
  selected?: boolean;
  required?: boolean;
  disabled?: boolean;
  read_only?: boolean;
  form_context?: SnapshotFormContext;
  element_id?: string;
  actions?: string[];
  actionable: boolean;
};

export type CachedFieldMapping = {
  canonicalField: CanonicalField;
  resumePath: string;
  confidence: number;
  source: "name" | "group" | "form_context" | "hint" | "resume_upload" | "section";
  choiceValue?: string | undefined;
};

export type ResolvedStructureNode = {
  node: SnapshotNode;
  key: string;
  mapping_id: string;
  mapping?: CachedFieldMapping;
};

export type FormMappingHint = {
  mapping_id: string;
  label?: string | undefined;
  canonicalField?: CanonicalField | undefined;
  resumePath?: string | undefined;
  choiceValue?: string | undefined;
};

const MAX_STRUCTURE_NODES = 256;
const MAX_MAPPING_CACHE_ENTRIES = 64;
const mappingCache = new Map<string, Map<string, CachedFieldMapping>>();

export type RepeatSectionKind = "education" | "experience" | "projects";

type SectionField = {
  canonicalField: CanonicalField;
  property: string;
};

export function repeatSectionKind(label: string): RepeatSectionKind | undefined {
  const normalized = normalizeLabel(label);
  if (/^(教育经历|教育背景|education)(\d+)?$/u.test(normalized)) return "education";
  if (
    /^(实习经历|工作经历|工作经验|internship|internshipexperience|workexperience|experience)(\d+)?$/u.test(
      normalized,
    )
  ) {
    return "experience";
  }
  if (
    /^(项目经历|项目经验|project|projects|projectexperience)(\d+)?$/u.test(normalized)
  ) {
    return "projects";
  }
  return undefined;
}

function explicitSectionIndex(label: string): number | undefined {
  const match = normalizeLabel(label).match(/(\d+)$/u);
  if (!match?.[1]) return undefined;
  const number = Number(match[1]);
  return Number.isSafeInteger(number) && number > 0 ? number - 1 : undefined;
}

function matchSectionField(
  kind: RepeatSectionKind,
  label: string,
): SectionField | undefined {
  const normalized = normalizeLabel(label);
  const matches = (...values: string[]) =>
    values.some((value) => normalized === normalizeLabel(value));

  if (kind === "education") {
    if (matches("学校", "院校", "school", "university")) {
      return { canonicalField: "university", property: "school" };
    }
    if (matches("学历", "学位", "degree")) {
      return { canonicalField: "degree", property: "degree" };
    }
    if (matches("专业", "major", "field of study")) {
      return { canonicalField: "major", property: "major" };
    }
    if (matches("入学时间", "开始时间", "start date")) {
      return { canonicalField: "education_start_date", property: "start_date" };
    }
    if (matches("毕业时间", "预计毕业时间", "结束时间", "graduation date", "end date")) {
      return { canonicalField: "graduation_date", property: "graduation_date" };
    }
    if (matches("GPA", "GPA / 绩点", "绩点", "grade point average")) {
      return { canonicalField: "gpa", property: "gpa" };
    }
  }

  if (kind === "experience") {
    if (matches("公司", "公司名称", "company")) {
      return { canonicalField: "current_company", property: "company" };
    }
    if (matches("职位", "岗位", "title", "job title")) {
      return { canonicalField: "current_title", property: "title" };
    }
    if (matches("地点", "工作地点", "location")) {
      return { canonicalField: "experience_location", property: "location" };
    }
    if (matches("开始时间", "start date")) {
      return { canonicalField: "experience_start_date", property: "start_date" };
    }
    if (matches("结束时间", "end date")) {
      return { canonicalField: "experience_end_date", property: "end_date" };
    }
    if (matches("工作内容", "工作职责", "职责", "summary", "description")) {
      return { canonicalField: "experience_summary", property: "summary" };
    }
  }

  if (kind === "projects") {
    if (matches("项目名称", "project name", "name")) {
      return { canonicalField: "project_name", property: "name" };
    }
    if (matches("项目角色", "角色", "project role", "role")) {
      return { canonicalField: "project_role", property: "role" };
    }
    if (matches("开始时间", "start date")) {
      return { canonicalField: "project_start_date", property: "start_date" };
    }
    if (matches("结束时间", "end date")) {
      return { canonicalField: "project_end_date", property: "end_date" };
    }
    if (matches("技术栈", "技术", "technologies", "tech stack")) {
      return { canonicalField: "project_technologies", property: "technologies" };
    }
    if (matches("项目描述", "项目介绍", "summary", "description")) {
      return { canonicalField: "project_summary", property: "summary" };
    }
    if (matches("项目链接", "project url", "url", "link")) {
      return { canonicalField: "project_url", property: "url" };
    }
  }
  return undefined;
}

export function normalizeLabel(value: string): string {
  return value.normalize("NFKC").toLowerCase().replace(/[\s_\-:：/()（）.]+/g, "");
}

const fieldKeywords: ReadonlyArray<{
  field: CanonicalField;
  exact: readonly string[];
  contains: readonly string[];
}> = [
  { field: "first_name", exact: ["firstname", "givenname", "名"], contains: ["firstname", "givenname"] },
  { field: "last_name", exact: ["lastname", "surname", "familyname", "姓"], contains: ["lastname", "surname", "familyname"] },
  { field: "gender", exact: ["gender", "sex", "性别"], contains: ["gender", "性别"] },
  { field: "birth_date", exact: ["dateofbirth", "birthdate", "birthday", "出生日期", "出生年月", "生日"], contains: ["dateofbirth", "birthdate", "出生日期", "出生年月"] },
  { field: "id_type", exact: ["idtype", "identificationtype", "证件类型"], contains: ["idtype", "证件类型"] },
  { field: "id_number", exact: ["idnumber", "identificationnumber", "身份证号", "身份证号码", "证件号码", "证件号"], contains: ["idnumber", "身份证号", "证件号码", "证件号"] },
  { field: "ethnicity", exact: ["ethnicity", "nationalityethnicity", "民族"], contains: ["ethnicity", "民族"] },
  { field: "political_status", exact: ["politicalstatus", "政治面貌"], contains: ["politicalstatus", "政治面貌"] },
  { field: "health_status", exact: ["healthstatus", "健康状况", "健康状态"], contains: ["healthstatus", "健康状况"] },
  { field: "native_place_province", exact: ["nativeplaceprovince", "籍贯省", "籍贯省份"], contains: ["nativeplaceprovince", "籍贯省"] },
  { field: "native_place_city", exact: ["nativeplacecity", "籍贯市", "籍贯城市"], contains: ["nativeplacecity", "籍贯市", "籍贯城市"] },
  { field: "native_place_district", exact: ["nativeplacedistrict", "籍贯区", "籍贯区县", "籍贯县"], contains: ["nativeplacedistrict", "籍贯区县"] },
  { field: "native_place", exact: ["nativeplace", "籍贯"], contains: ["nativeplace", "籍贯"] },
  { field: "household_registration_province", exact: ["householdregistrationprovince", "hukouprovince", "户籍省", "户籍省份", "户口省份"], contains: ["householdregistrationprovince", "hukouprovince", "户籍省", "户口省"] },
  { field: "household_registration_city", exact: ["householdregistrationcity", "hukoucity", "户籍市", "户籍城市", "户口城市"], contains: ["householdregistrationcity", "hukoucity", "户籍市", "户口城市"] },
  { field: "household_registration_district", exact: ["householdregistrationdistrict", "hukoudistrict", "户籍区", "户籍区县", "户口区县"], contains: ["householdregistrationdistrict", "户籍区县"] },
  { field: "household_registration", exact: ["householdregistration", "hukou", "户籍所在地", "户口所在地", "户籍"], contains: ["householdregistration", "hukou", "户籍", "户口"] },
  { field: "student_origin_province", exact: ["studentoriginprovince", "生源地省", "生源省份", "高考生源地省", "高考生源省份"], contains: ["studentoriginprovince", "生源地省", "生源省", "高考生源省"] },
  { field: "student_origin_city", exact: ["studentorigincity", "生源地市", "生源城市", "高考生源地市", "高考生源城市"], contains: ["studentorigincity", "生源地市", "生源城市", "高考生源城市"] },
  { field: "student_origin_district", exact: ["studentorigindistrict", "生源地区县", "生源地县", "高考生源地区县"], contains: ["studentorigindistrict", "生源地区县"] },
  { field: "student_origin", exact: ["studentorigin", "sourceplace", "生源地", "生源所在地", "生源地区", "高考生源地"], contains: ["studentorigin", "sourceplace", "生源地", "生源所在", "高考生源"] },
  { field: "is_fresh_graduate", exact: ["isfreshgraduate", "freshgraduate", "是否为应届毕业生", "是否应届毕业生", "应届毕业生"], contains: ["freshgraduate", "应届毕业生"] },
  { field: "marital_status", exact: ["maritalstatus", "婚姻状况", "婚姻状态"], contains: ["maritalstatus", "婚姻"] },
  { field: "height_cm", exact: ["heightcm", "身高cm", "身高厘米", "身高"], contains: [] },
  { field: "weight_kg", exact: ["weightkg", "体重kg", "体重千克", "体重公斤", "体重"], contains: [] },
  { field: "full_name", exact: ["name", "fullname", "姓名"], contains: ["fullname", "candidatename"] },
  { field: "email", exact: ["email", "emailaddress", "邮箱", "电子邮箱"], contains: ["email"] },
  { field: "phone", exact: ["phone", "phonenumber", "mobile", "mobilenumber", "手机号", "手机", "手机号码", "联系电话", "电话"], contains: ["phone", "mobile", "手机号", "联系电话"] },
  { field: "current_residence_province", exact: ["currentresidenceprovince", "现居住省份", "现居省份", "当前居住省份", "province", "省份", "省"], contains: ["currentresidenceprovince", "现居住省", "当前居住省"] },
  { field: "city", exact: ["city", "location", "currentlocation", "所在城市", "当前城市", "现居住城市", "现居城市", "城市", "现居住地", "现居地", "居住地"], contains: ["currentlocation", "现居住城市", "现居城市", "当前城市"] },
  { field: "district", exact: ["district", "county", "区", "区县", "现居住区县", "现居区县"], contains: ["现居住区县", "现居区县"] },
  { field: "address", exact: ["address", "mailingaddress", "地址", "通讯地址"], contains: ["address", "通讯地址"] },
  { field: "education_province", exact: ["educationprovince", "schoolprovince", "院校所在省份", "学校所在省份", "就读院校所在省份"], contains: ["educationprovince", "schoolprovince", "院校所在省", "学校所在省"] },
  { field: "education_city", exact: ["educationcity", "schoolcity", "院校所在城市", "学校所在城市", "就读院校所在城市"], contains: ["educationcity", "schoolcity", "院校所在城市", "学校所在城市"] },
  { field: "university", exact: ["school", "university", "college", "学校", "院校", "毕业院校"], contains: ["school", "university", "college", "毕业院校"] },
  { field: "degree", exact: ["degree", "educationlevel", "学历", "学位"], contains: ["degree", "educationlevel", "学历", "学位"] },
  {
    field: "major",
    exact: ["discipline", "major", "fieldofstudy", "专业", "专业名称", "所学专业", "主修专业", "专业方向"],
    contains: ["discipline", "major", "fieldofstudy", "所学专业", "主修专业"],
  },
  { field: "education_start_date", exact: ["educationstartdate", "入学时间", "入学日期"], contains: ["educationstartdate", "入学时间", "入学日期"] },
  { field: "graduation_date", exact: ["graduationdate", "graduationtime", "enddate", "毕业时间", "毕业日期", "预计毕业时间", "预计毕业日期"], contains: ["graduation", "毕业时间", "毕业日期"] },
  { field: "gpa", exact: ["gpa", "gradepointaverage", "绩点"], contains: ["gpa", "绩点"] },
  { field: "current_company", exact: ["currentcompany", "company", "当前公司", "公司"], contains: ["currentcompany", "当前公司"] },
  { field: "current_title", exact: ["currenttitle", "jobtitle", "title", "职位", "当前职位"], contains: ["currenttitle", "jobtitle", "当前职位"] },
  { field: "linkedin", exact: ["linkedin", "linkedinurl", "linkedinprofile"], contains: ["linkedin"] },
  { field: "github", exact: ["github", "githuburl"], contains: ["github"] },
  { field: "portfolio", exact: ["portfolio", "website", "personalwebsite", "个人主页", "个人网站"], contains: ["portfolio", "personalwebsite", "个人主页", "个人网站"] },
  { field: "cover_letter", exact: ["coverletter", "additionalinformation", "additionalinfo", "motivation", "selfintroduction", "自我介绍", "补充信息", "求职动机"], contains: ["coverletter", "additionalinformation", "selfintroduction", "自我介绍", "补充信息", "求职动机"] },
  { field: "accept_transfer", exact: ["是否接受岗位调剂", "是否接受调剂", "接受岗位调剂", "accepttransfer", "willingtotransfer"], contains: ["岗位调剂", "接受调剂", "accepttransfer", "willingtotransfer"] },
  { field: "preferred_locations", exact: ["preferredlocation", "preferredlocations", "意向地点", "意向城市", "期望工作地点", "意向工作地点", "期望工作城市"], contains: ["preferredlocation", "意向工作地点", "期望工作地点"] },
  { field: "available_date", exact: ["availabledate", "到岗时间", "到岗日期", "最早到岗时间"], contains: ["availabledate", "到岗时间", "到岗日期"] },
];

export function matchField(
  label: string,
): { canonicalField: CanonicalField; confidence: number } | undefined {
  const normalized = normalizeLabel(label);
  if (!normalized) return undefined;
  for (const rule of fieldKeywords) {
    if (rule.exact.includes(normalized)) {
      return { canonicalField: rule.field, confidence: 0.99 };
    }
  }
  for (const rule of fieldKeywords) {
    if (rule.contains.some((keyword) => normalized.includes(normalizeLabel(keyword)))) {
      return { canonicalField: rule.field, confidence: 0.88 };
    }
  }
  return undefined;
}

function matchFieldExact(
  label: string,
): { canonicalField: CanonicalField; confidence: number } | undefined {
  const normalized = normalizeLabel(label);
  if (!normalized) return undefined;
  for (const rule of fieldKeywords) {
    if (rule.exact.includes(normalized)) {
      return { canonicalField: rule.field, confidence: 0.99 };
    }
  }
  return undefined;
}

export function isUploadControl(node: SnapshotNode): boolean {
  const role = node.role.toLowerCase();
  return (
    node.actions?.includes("upload_file") === true ||
    (role === "button" && normalizeLabel(node.value ?? "").includes("未选择任何文件"))
  );
}

export function isResumeUpload(node: SnapshotNode): boolean {
  if (!isUploadControl(node)) return false;

  const label = normalizeLabel(
    [
      node.name,
      node.description ?? "",
      node.group_label ?? "",
      node.form_context?.section_label ?? "",
      node.form_context?.nearby_label ?? "",
      node.form_context?.placeholder ?? "",
      node.form_context?.html_name ?? "",
    ].join(" "),
  );
  return (
    label.includes("resume") ||
    label.includes("cv") ||
    label.includes("简历")
  );
}

const choiceAliasGroups = [
  ["是", "yes", "true", "1", "接受", "accept"],
  ["否", "no", "false", "0", "不接受", "decline"],
  ["男", "男性", "male"],
  ["女", "女性", "female"],
] as const;

export function choiceMatches(option: string, desired: string): boolean {
  const normalizedOption = normalizeLabel(option);
  const normalizedDesired = normalizeLabel(desired);
  if (!normalizedOption || !normalizedDesired) return false;
  if (normalizedOption === normalizedDesired) return true;
  return choiceAliasGroups.some((group) => {
    const normalized = group.map((item) => normalizeLabel(item));
    return normalized.includes(normalizedOption) && normalized.includes(normalizedDesired);
  });
}

export function booleanChoice(value: string): boolean | undefined {
  if (choiceMatches(value, "true")) return true;
  if (choiceMatches(value, "false")) return false;
  return undefined;
}

export function isChoiceControl(node: SnapshotNode): boolean {
  return ["radio", "checkbox", "switch", "option", "menuitemradio", "menuitemcheckbox"].includes(node.role.toLowerCase())
    || (node.form_context?.dom_tag.toLowerCase() === "input"
      && ["radio", "checkbox"].includes(node.form_context?.input_type?.toLowerCase() ?? ""));
}

export function choiceState(node: SnapshotNode): boolean | undefined {
  const checked = node.checked === "true" ? true : node.checked === "false" ? false : undefined;
  if (node.checked !== undefined && checked === undefined) return;
  if (checked !== undefined && node.selected !== undefined && checked !== node.selected) return;
  return checked ?? node.selected;
}

function isStructureNode(node: SnapshotNode): boolean {
  const role = node.role.toLowerCase();
  if (node.actionable && isUploadControl(node)) return true;
  if (isChoiceControl(node)) return true;
  if (node.actions?.some(action => ["select_choice", "set_date"].includes(action))) return true;
  return [
    "textbox",
    "searchbox",
    "spinbutton",
    "combobox",
    "listbox",
    "radio",
    "checkbox",
    "datetime",
    "date",
    "time",
  ].includes(role);
}

function structuralBase(node: SnapshotNode): string {
  return JSON.stringify([
    normalizeLabel(node.role),
    normalizeLabel(node.name),
    normalizeLabel(node.group_role ?? ""),
    normalizeLabel(node.group_label ?? ""),
    normalizeLabel(node.description ?? ""),
    node.actionable ? 1 : 0,
  ]);
}

function mappingIdForKey(key: string): string {
  return createHash("sha256").update(key).digest("hex").slice(0, 24);
}

function structuralEntries(
  nodes: readonly SnapshotNode[],
): Array<{ node: SnapshotNode; key: string; mapping_id: string }> {
  const counts = new Map<string, number>();
  const entries: Array<{ node: SnapshotNode; key: string; mapping_id: string }> = [];
  for (const node of nodes.slice(0, MAX_STRUCTURE_NODES)) {
    if (!isStructureNode(node)) continue;
    const base = structuralBase(node);
    const occurrence = counts.get(base) ?? 0;
    counts.set(base, occurrence + 1);
    const key = `${base}#${occurrence}`;
    entries.push({ node, key, mapping_id: mappingIdForKey(key) });
  }
  return entries;
}

export function formStructureSignature(nodes: readonly SnapshotNode[]): string {
  const bases = structuralEntries(nodes).map(({ key }) => key);
  return createHash("sha256")
    .update(JSON.stringify(bases))
    .digest("hex")
    .slice(0, 24);
}

function regionAxisField(group: CanonicalField, label: string): CanonicalField | undefined {
  const normalized = normalizeLabel(label);
  const axis = ["province", "省", "省份"].includes(normalized) ? 0
    : ["city", "市", "城市"].includes(normalized) ? 1
    : ["district", "county", "区", "区县", "县"].includes(normalized) ? 2 : undefined;
  if (axis === undefined) return;
  const fields: Partial<Record<CanonicalField, readonly CanonicalField[]>> = {
    native_place: ["native_place_province", "native_place_city", "native_place_district"],
    household_registration: ["household_registration_province", "household_registration_city", "household_registration_district"],
    student_origin: ["student_origin_province", "student_origin_city", "student_origin_district"],
    city: ["current_residence_province", "city", "district"],
    address: ["current_residence_province", "city", "district"],
  };
  return fields[group]?.[axis];
}

function deriveMappings(
  entries: ReadonlyArray<{ node: SnapshotNode; key: string; mapping_id: string }>,
): Map<string, CachedFieldMapping> {
  const mappings = new Map<string, CachedFieldMapping>();
  const groupIndexes = new Map<string, number>();
  const nextIndexes: Record<RepeatSectionKind, number> = {
    education: 0,
    experience: 0,
    projects: 0,
  };

  // Count model-visible controls sharing the same semantic group. Frameworks often
  // implement one scalar select with several internal DOM controls; DOM group size
  // alone cannot distinguish those internals from a real composite field such as
  // province + city. A single projected control can safely use an exact group label,
  // while two or more projected controls keep the group observable but unmapped.
  const hasDataAction = (node: SnapshotNode): boolean =>
    ["input_text", "select_option", "set_value", "upload_file", "select_choice", "set_date"].some(
      (action) => node.actions?.includes(action) === true,
    );
  const projectedGroupDataCounts = new Map<string, number>();
  for (const { node } of entries) {
    if (!hasDataAction(node)) continue;
    const label = normalizeLabel(
      node.group_label ?? node.form_context?.group_label ?? "",
    );
    if (!label) continue;
    projectedGroupDataCounts.set(
      label,
      (projectedGroupDataCounts.get(label) ?? 0) + 1,
    );
  }

  for (const { node, key } of entries) {
    if (node.actionable && isResumeUpload(node)) {
      mappings.set(key, {
        canonicalField: "resume_path",
        resumePath: resumePathForCanonicalField("resume_path"),
        confidence: 1,
        source: "resume_upload",
      });
      continue;
    }

    const sectionLabel = node.group_label || node.form_context?.section_label;
    const sectionKind = sectionLabel
      ? repeatSectionKind(sectionLabel)
      : undefined;
    if (sectionKind && sectionLabel) {
      const explicitIndex = explicitSectionIndex(sectionLabel);
      const groupKey =
        node.group_id ?? `${sectionKind}:${normalizeLabel(sectionLabel)}`;
      let sectionIndex = explicitIndex ?? groupIndexes.get(groupKey);
      if (sectionIndex === undefined) {
        sectionIndex = nextIndexes[sectionKind];
        nextIndexes[sectionKind] += 1;
      } else {
        nextIndexes[sectionKind] = Math.max(nextIndexes[sectionKind], sectionIndex + 1);
      }
      groupIndexes.set(groupKey, sectionIndex);

      const sectionField = matchSectionField(
        sectionKind,
        node.name ||
          node.form_context?.placeholder ||
          node.form_context?.html_name ||
          "",
      );
      if (sectionField) {
        mappings.set(key, {
          canonicalField: sectionField.canonicalField,
          resumePath: `${sectionKind}[${sectionIndex}].${sectionField.property}`,
          confidence: 1,
          source: "section",
        });
        continue;
      }
    }

    const choiceGroupLabel = node.group_label ?? node.form_context?.group_label;
    const groupMatch = isChoiceControl(node) && choiceGroupLabel ? matchField(choiceGroupLabel) : undefined;
    if (groupMatch) {
      mappings.set(key, {
        ...groupMatch,
        resumePath: resumePathForCanonicalField(groupMatch.canonicalField),
        source: "group",
      });
      continue;
    }

    const regionGroup = choiceGroupLabel ? matchFieldExact(choiceGroupLabel) : undefined;
    const regionField = regionGroup && hasDataAction(node)
      ? regionAxisField(regionGroup.canonicalField, node.name || node.form_context?.placeholder || node.form_context?.html_name || "")
      : undefined;
    if (regionField) {
      mappings.set(key, { canonicalField: regionField,
        resumePath: resumePathForCanonicalField(regionField), confidence: 1, source: "group" });
      continue;
    }

    const evidenceLabels: ReadonlyArray<{
      label: string;
      source: CachedFieldMapping["source"];
      exactOnly?: boolean;
    }> = [
      { label: node.name, source: "name" },
      { label: node.description ?? "", source: "group" },
      {
        label: (() => {
          const groupLabel =
            node.group_label ?? node.form_context?.group_label ?? "";
          if (!groupLabel) return "";
          return hasDataAction(node) &&
            (projectedGroupDataCounts.get(normalizeLabel(groupLabel)) ?? 0) <= 1
            ? groupLabel
            : "";
        })(),
        source: "group",
        exactOnly: true,
      },
      { label: node.form_context?.nearby_label ?? "", source: "form_context" },
      { label: node.form_context?.placeholder ?? "", source: "form_context" },
      {
        label: node.form_context?.html_name ?? "",
        source: "form_context",
        exactOnly: true,
      },
      {
        label: node.form_context?.autocomplete ?? "",
        source: "form_context",
        exactOnly: true,
      },
    ];
    for (const evidence of evidenceLabels) {
      const match = evidence.exactOnly
        ? matchFieldExact(evidence.label)
        : matchField(evidence.label);
      if (!match) continue;
      mappings.set(key, {
        ...match,
        resumePath: resumePathForCanonicalField(match.canonicalField),
        source: evidence.source,
      });
      break;
    }
  }
  return mappings;
}

function cacheKeyFor(scope: string, signature: string): string {
  return scope.length > 0 ? `${scope}\u0000${signature}` : signature;
}

function remember(cacheKey: string, mappings: Map<string, CachedFieldMapping>): void {
  if (mappingCache.has(cacheKey)) {
    mappingCache.delete(cacheKey);
  }
  mappingCache.set(cacheKey, mappings);
  while (mappingCache.size > MAX_MAPPING_CACHE_ENTRIES) {
    const oldest = mappingCache.keys().next().value as string | undefined;
    if (oldest === undefined) break;
    mappingCache.delete(oldest);
  }
}

export function resolveFormMappings(
  nodes: readonly SnapshotNode[],
  hints: readonly FormMappingHint[] = [],
  cacheScope = "",
): {
  signature: string;
  cacheHit: boolean;
  cacheEntries: number;
  nodes: ResolvedStructureNode[];
} {
  const entries = structuralEntries(nodes);
  const signature = createHash("sha256")
    .update(JSON.stringify(entries.map(({ key }) => key)))
    .digest("hex")
    .slice(0, 24);
  const cacheKey = cacheKeyFor(cacheScope, signature);
  const cached = mappingCache.get(cacheKey);
  const cacheHit = cached !== undefined;
  // Re-derive automatic mappings on every observation so newly available Browser
  // semantics (for example form_context) take effect without changing the stable
  // structure signature. Explicit taught mappings always win; otherwise cached
  // automatic evidence is only a fallback when the fresh observation cannot map.
  const mappings = deriveMappings(entries);
  for (const [key, mapping] of cached ?? []) {
    if (mapping.source === "hint" || !mappings.has(key)) mappings.set(key, mapping);
  }

  const entryByMappingId = new Map(entries.map((entry) => [entry.mapping_id, entry]));
  for (const hint of hints) {
    const entry = entryByMappingId.get(hint.mapping_id);
    if (!entry) continue;
    const matched = hint.canonicalField
      ? { canonicalField: hint.canonicalField, confidence: 1 }
      : hint.label
        ? matchField(hint.label)
        : undefined;
    if (!matched) continue;
    mappings.set(entry.key, {
      canonicalField: matched.canonicalField,
      resumePath:
        hint.resumePath ?? resumePathForCanonicalField(matched.canonicalField),
      confidence: matched.confidence,
      source: "hint",
      ...(hint.choiceValue === undefined ? {} : { choiceValue: hint.choiceValue }),
    });
  }
  remember(cacheKey, mappings);

  return {
    signature,
    cacheHit,
    cacheEntries: mappingCache.size,
    nodes: entries.map(({ node, key, mapping_id }) => {
      const mapping = mappings.get(key);
      return mapping === undefined
        ? { node, key, mapping_id }
        : { node, key, mapping_id, mapping };
    }),
  };
}
