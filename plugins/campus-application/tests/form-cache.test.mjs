import assert from "node:assert/strict";
import test from "node:test";
import {
  formStructureSignature,
  isResumeUpload,
  matchField,
  resolveFormMappings,
} from "../dist/form-cache.js";

function fixture(elementSuffix, currentName, checked) {
  return [
    {
      role: "textbox",
      name: "姓名",
      value: currentName,
      element_id: `element_name_${elementSuffix}`,
      actionable: true,
    },
    {
      role: "radio",
      name: " 是",
      group_id: `group_${elementSuffix}`,
      group_role: "group",
      group_label: "是否接受岗位调剂",
      checked: "false",
      element_id: `element_yes_${elementSuffix}`,
      actionable: true,
    },
    {
      role: "radio",
      name: " 否",
      group_id: `group_${elementSuffix}`,
      group_role: "group",
      group_label: "是否接受岗位调剂",
      checked,
      element_id: `element_no_${elementSuffix}`,
      actionable: true,
    },
  ];
}

test("structure signature ignores volatile ids, values, and checked state", () => {
  const before = fixture("a", "", "false");
  const after = fixture("b", "示例候选人", "true");
  assert.equal(formStructureSignature(before), formStructureSignature(after));
});

test("structure signature ignores passive group text that reflects current control values", () => {
  const before = [
    ...fixture("passive-a", "", "false"),
    {
      role: "StaticText",
      name: "----",
      group_id: "group_education_a",
      group_role: "group",
      group_label: "教育经历 1",
      actionable: false,
    },
  ];
  const after = [
    ...fixture("passive-b", "示例候选人", "true"),
    {
      role: "StaticText",
      name: "2027",
      group_id: "group_education_b",
      group_role: "group",
      group_label: "教育经历 1",
      actionable: false,
    },
  ];

  assert.equal(formStructureSignature(before), formStructureSignature(after));
});

test("structure signature ignores unrelated actionable buttons and picker affordances", () => {
  const fields = fixture("buttons", "", "false");
  const withAuxiliaryButtons = [
    ...fields,
    { role: "button", name: "保存并进入下一步", actionable: true },
    {
      role: "button",
      name: "显示月份选择器 显示月份选择器",
      group_id: "group_education",
      group_role: "group",
      group_label: "教育经历 1",
      actionable: true,
    },
  ];

  assert.equal(
    formStructureSignature(fields),
    formStructureSignature(withAuxiliaryButtons),
  );
});

test("bounded mapping cache reuses mappings for the same structure", () => {
  const before = fixture("cache-a", "", "false");
  const after = fixture("cache-b", "示例候选人", "true");

  const first = resolveFormMappings(before);
  const second = resolveFormMappings(after);

  assert.equal(first.signature, second.signature);
  assert.equal(second.cacheHit, true);
  assert.ok(second.cacheEntries >= 1);
  assert.equal(
    second.nodes.find(({ node }) => node.name === "姓名")?.mapping?.canonicalField,
    "full_name",
  );
  assert.equal(
    second.nodes.find(({ node }) => node.name.trim() === "否")?.mapping?.canonicalField,
    "accept_transfer",
  );
});

test("repeated section mappings target indexed resume paths", () => {
  const nodes = [
    {
      role: "textbox",
      name: "学校",
      group_id: "group_edu_1",
      group_role: "group",
      group_label: "教育经历 1",
      element_id: "element_edu1_school",
      actionable: true,
    },
    {
      role: "DateTime",
      name: "入学时间",
      group_id: "group_edu_1",
      group_role: "group",
      group_label: "教育经历 1",
      element_id: "element_edu1_start",
      actionable: true,
    },
    {
      role: "textbox",
      name: "学校",
      group_id: "group_edu_2",
      group_role: "group",
      group_label: "教育经历 2",
      element_id: "element_edu2_school",
      actionable: true,
    },
    {
      role: "textbox",
      name: "公司",
      group_id: "group_exp_2",
      group_role: "group",
      group_label: "实习经历 2",
      element_id: "element_exp2_company",
      actionable: true,
    },
    {
      role: "textbox",
      name: "技术栈",
      group_id: "group_project_2",
      group_role: "group",
      group_label: "项目经历 2",
      element_id: "element_project2_tech",
      actionable: true,
    },
  ];

  const resolved = resolveFormMappings(nodes);
  const byElement = new Map(
    resolved.nodes.map(({ node, mapping }) => [node.element_id, mapping]),
  );

  assert.equal(byElement.get("element_edu1_school")?.resumePath, "education[0].school");
  assert.equal(byElement.get("element_edu1_start")?.resumePath, "education[0].start_date");
  assert.equal(byElement.get("element_edu2_school")?.resumePath, "education[1].school");
  assert.equal(byElement.get("element_exp2_company")?.resumePath, "experience[1].company");
  assert.equal(byElement.get("element_project2_tech")?.resumePath, "projects[1].technologies");
  assert.equal(byElement.get("element_project2_tech")?.canonicalField, "project_technologies");
  assert.equal(byElement.get("element_project2_tech")?.source, "section");
});

test("unnumbered repeated groups use stable document-order indexes", () => {
  const nodes = [
    {
      role: "textbox",
      name: "学校",
      group_id: "group_a",
      group_role: "group",
      group_label: "教育经历",
      element_id: "element_school_a",
      actionable: true,
    },
    {
      role: "textbox",
      name: "专业",
      group_id: "group_a",
      group_role: "group",
      group_label: "教育经历",
      element_id: "element_major_a",
      actionable: true,
    },
    {
      role: "textbox",
      name: "学校",
      group_id: "group_b",
      group_role: "group",
      group_label: "教育经历",
      element_id: "element_school_b",
      actionable: true,
    },
  ];

  const resolved = resolveFormMappings(nodes);
  const byElement = new Map(
    resolved.nodes.map(({ node, mapping }) => [node.element_id, mapping]),
  );

  assert.equal(byElement.get("element_school_a")?.resumePath, "education[0].school");
  assert.equal(byElement.get("element_major_a")?.resumePath, "education[0].major");
  assert.equal(byElement.get("element_school_b")?.resumePath, "education[1].school");
});

test("Chinese campus location fields keep native place and student origin distinct", () => {
  assert.equal(matchField("籍贯")?.canonicalField, "native_place");
  assert.equal(matchField("生源地")?.canonicalField, "student_origin");
  assert.equal(matchField("高考生源地")?.canonicalField, "student_origin");
  assert.equal(matchField("籍贯省份")?.canonicalField, "native_place_province");
  assert.equal(matchField("生源地市")?.canonicalField, "student_origin_city");
  assert.equal(matchField("就读院校所在城市")?.canonicalField, "education_city");
  assert.equal(matchField("现居住城市")?.canonicalField, "city");
  assert.equal(matchField("健康状况")?.canonicalField, "health_status");
  assert.equal(matchField("是否为应届毕业生")?.canonicalField, "is_fresh_graduate");
});

test("resume upload detection requires real upload semantics and explicit resume context", () => {
  assert.equal(
    isResumeUpload({
      role: "button",
      name: "本人同意针对相关工作机会提供简历及后续招聘进程中所需的个人信息",
      actionable: true,
      actions: ["click"],
    }),
    false,
  );
  assert.equal(
    isResumeUpload({
      role: "link",
      name: "增加更多 简历",
      actionable: true,
      actions: ["click"],
    }),
    false,
  );
  assert.equal(
    isResumeUpload({
      role: "button",
      name: "选择文件",
      group_label: "简历",
      value: "未选择任何文件",
      actionable: true,
      actions: ["upload_file"],
    }),
    true,
  );
});

test("stable mapping hints teach unlabeled controls across volatile element ids", () => {
  const first = resolveFormMappings([
    {
      role: "textbox",
      name: "",
      element_id: "element_unlabeled_a",
      actionable: true,
      actions: ["input_text"],
    },
  ]);
  const mappingId = first.nodes[0]?.mapping_id;
  assert.ok(mappingId);
  assert.equal(first.nodes[0]?.mapping, undefined);

  const taught = resolveFormMappings(
    [
      {
        role: "textbox",
        name: "",
        element_id: "element_unlabeled_b",
        actionable: true,
        actions: ["input_text"],
      },
    ],
    [{ mapping_id: mappingId, label: "姓名" }],
  );
  assert.equal(taught.nodes[0]?.mapping?.canonicalField, "full_name");

  const reused = resolveFormMappings([
    {
      role: "textbox",
      name: "",
      element_id: "element_unlabeled_c",
      actionable: true,
      actions: ["input_text"],
    },
  ]);
  assert.equal(reused.cacheHit, true);
  assert.equal(reused.nodes[0]?.mapping?.canonicalField, "full_name");
});

test("mapping cache scopes identical unlabeled structures by site", () => {
  const nodes = [
    {
      role: "textbox",
      name: "",
      element_id: "element_scoped_a",
      actionable: true,
      actions: ["input_text"],
    },
  ];
  const first = resolveFormMappings(nodes, [], "site-a.example");
  const mappingId = first.nodes[0]?.mapping_id;
  assert.ok(mappingId);

  const taught = resolveFormMappings(
    nodes,
    [{ mapping_id: mappingId, canonicalField: "full_name" }],
    "site-a.example",
  );
  assert.equal(taught.nodes[0]?.mapping?.canonicalField, "full_name");

  const otherSite = resolveFormMappings(nodes, [], "site-b.example");
  assert.equal(otherSite.cacheHit, false);
  assert.equal(otherSite.nodes[0]?.mapping, undefined);
});

test("stable mapping hints can preserve a learned choice value for unlabeled radios", () => {
  const first = resolveFormMappings([
    {
      role: "radio",
      name: "",
      element_id: "element_gender_male_a",
      actionable: true,
      actions: ["click"],
    },
  ]);
  const mappingId = first.nodes[0]?.mapping_id;
  assert.ok(mappingId);

  const taught = resolveFormMappings(
    [
      {
        role: "radio",
        name: "",
        element_id: "element_gender_male_b",
        actionable: true,
        actions: ["click"],
      },
    ],
    [{
      mapping_id: mappingId,
      canonicalField: "gender",
      choiceValue: "男",
    }],
  );
  assert.equal(taught.nodes[0]?.mapping?.canonicalField, "gender");
  assert.equal(taught.nodes[0]?.mapping?.choiceValue, "男");
});

test("form context maps unlabeled fields without changing stable mapping identity", () => {
  const base = {
    role: "combobox",
    name: "",
    element_id: "element_origin_a",
    actionable: true,
    actions: ["select_option"],
  };
  const withContext = {
    ...base,
    element_id: "element_origin_b",
    form_context: {
      field_signature: "0123456789abcdef01234567",
      dom_tag: "select",
      html_name: "studentOrigin",
      section_label: "基本信息",
      component_hint: "ant-select",
      option_count: 34,
    },
  };

  assert.equal(
    formStructureSignature([base]),
    formStructureSignature([withContext]),
    "new form metadata must not invalidate existing mapping memory",
  );

  const scope = "form-context-upgrade.example";
  const plain = resolveFormMappings([base], [], scope);
  const enriched = resolveFormMappings([withContext], [], scope);
  assert.equal(plain.nodes[0]?.mapping, undefined);
  assert.equal(
    enriched.cacheHit,
    true,
    "a prior empty cache entry must not suppress newly observed form semantics",
  );
  assert.equal(enriched.nodes[0]?.mapping?.canonicalField, "student_origin");
  assert.equal(enriched.nodes[0]?.mapping?.source, "form_context");
});

test("machine-oriented form names require exact canonical matches", () => {
  const phoneArea = resolveFormMappings(
    [{
      role: "combobox",
      name: "",
      value: "中国大陆",
      element_id: "element_phone_area",
      actionable: true,
      actions: ["select_option"],
      form_context: {
        field_signature: "1234567890abcdef12345678",
        dom_tag: "select",
        html_name: "phoneArea",
        option_count: 2,
      },
    }],
    [],
    "machine-name-phone-area.example",
  );
  assert.equal(
    phoneArea.nodes[0]?.mapping,
    undefined,
    "phoneArea is a country/area selector and must not inherit the phone mapping by substring",
  );

  const exactPhone = resolveFormMappings(
    [{
      role: "textbox",
      name: "",
      element_id: "element_phone",
      actionable: true,
      actions: ["input_text"],
      form_context: {
        field_signature: "abcdef1234567890abcdef12",
        dom_tag: "input",
        html_name: "phone",
      },
    }],
    [],
    "machine-name-phone.example",
  );
  assert.equal(exactPhone.nodes[0]?.mapping?.canonicalField, "phone");
  assert.equal(exactPhone.nodes[0]?.mapping?.source, "form_context");
});

test("broad semantic matching does not confuse professional-detail fields with major", () => {
  assert.equal(matchField("专业")?.canonicalField, "major");
  assert.equal(matchField("专业名称")?.canonicalField, "major");
  assert.equal(matchField("所学专业")?.canonicalField, "major");
  assert.equal(matchField("专业课程"), undefined);
  assert.equal(matchField("专业资格证书"), undefined);
  assert.equal(matchField("专业资格证书等级"), undefined);
  assert.equal(matchField("专业资格证书获得时间"), undefined);
});

test("nearby labels can identify resume uploads without relabeling other attachments", () => {
  const resume = {
    role: "button",
    name: "选择文件",
    value: "未选择任何文件",
    element_id: "element_resume_upload",
    actionable: true,
    actions: ["upload_file"],
    form_context: {
      field_signature: "333333333333333333333333",
      dom_tag: "input",
      input_type: "file",
      nearby_label: "简历",
    },
  };
  const transcript = {
    ...resume,
    element_id: "element_transcript_upload",
    form_context: {
      ...resume.form_context,
      field_signature: "444444444444444444444444",
      nearby_label: "成绩单",
    },
  };
  assert.equal(isResumeUpload(resume), true);
  assert.equal(isResumeUpload(transcript), false);
});

test("one projected control may use an exact shared group label despite framework internals", () => {
  const scalar = resolveFormMappings(
    [{
      role: "combobox",
      name: "",
      group_label: "证件类型",
      element_id: "element_id_type",
      actionable: true,
      actions: ["select_option"],
      form_context: {
        field_signature: "555555555555555555555555",
        dom_tag: "select",
        html_name: "field_1",
        group_label: "证件类型",
        group_index: 2,
        group_size: 3,
      },
    }],
    [],
    "scalar-framework-group.example",
  );
  assert.equal(scalar.nodes[0]?.mapping?.canonicalField, "id_type");
  assert.equal(scalar.nodes[0]?.mapping?.source, "group");
});

test("click-only framework wrappers do not block one exact data-bearing group control", () => {
  const resolved = resolveFormMappings(
    [
      {
        role: "combobox",
        name: "",
        group_label: "健康状况",
        element_id: "element_health_wrapper",
        actionable: true,
        actions: ["click"],
        form_context: {
          field_signature: "666666666666666666666666",
          dom_tag: "div",
          group_label: "健康状况",
          group_index: 0,
          group_size: 3,
        },
      },
      {
        role: "combobox",
        name: "",
        group_label: "健康状况",
        element_id: "element_health_select",
        actionable: true,
        actions: ["select_option"],
        form_context: {
          field_signature: "777777777777777777777777",
          dom_tag: "select",
          html_name: "field_health",
          group_label: "健康状况",
          group_index: 2,
          group_size: 3,
        },
      },
    ],
    [],
    "framework-wrapper.example",
  );
  assert.equal(resolved.nodes[0]?.mapping, undefined);
  assert.equal(resolved.nodes[1]?.mapping?.canonicalField, "health_status");
  assert.equal(resolved.nodes[1]?.mapping?.source, "group");
});

test("generic multi-control group labels stay observable without unsafe composite auto-mapping", () => {
  const composite = resolveFormMappings(
    [
      {
        role: "combobox",
        name: "",
        group_label: "籍贯",
        element_id: "element_native_place_province",
        actionable: true,
        actions: ["select_option"],
        form_context: {
          field_signature: "000000000000000000000001",
          dom_tag: "select",
          html_name: "field_1",
          group_label: "籍贯",
          group_index: 0,
          group_size: 2,
        },
      },
      {
        role: "combobox",
        name: "",
        group_label: "籍贯",
        element_id: "element_native_place_city",
        actionable: true,
        actions: ["select_option"],
        form_context: {
          field_signature: "000000000000000000000002",
          dom_tag: "select",
          html_name: "field_2",
          group_label: "籍贯",
          group_index: 1,
          group_size: 2,
        },
      },
    ],
    [],
    "composite-group.example",
  );
  assert.equal(composite.nodes[0]?.mapping, undefined);
  assert.equal(composite.nodes[1]?.mapping, undefined);

  const choices = resolveFormMappings(
    [
      {
        role: "radio",
        name: "男",
        group_label: "性别",
        element_id: "element_gender_male",
        actionable: true,
        actions: ["click"],
        form_context: {
          field_signature: "111111111111111111111111",
          dom_tag: "input",
          input_type: "radio",
          group_label: "性别",
          group_index: 0,
          group_size: 2,
        },
      },
      {
        role: "radio",
        name: "女",
        group_label: "性别",
        element_id: "element_gender_female",
        actionable: true,
        actions: ["click"],
        form_context: {
          field_signature: "222222222222222222222222",
          dom_tag: "input",
          input_type: "radio",
          group_label: "性别",
          group_index: 1,
          group_size: 2,
        },
      },
    ],
    [],
    "choice-group.example",
  );
  assert.equal(choices.nodes[0]?.mapping?.canonicalField, "gender");
  assert.equal(choices.nodes[1]?.mapping?.canonicalField, "gender");
});

test("form context section labels preserve repeated education paths", () => {
  const resolved = resolveFormMappings(
    [{
      role: "textbox",
      name: "",
      element_id: "element_school_a",
      actionable: true,
      actions: ["input_text"],
      form_context: {
        field_signature: "fedcba9876543210fedcba98",
        dom_tag: "input",
        html_name: "school",
        section_label: "教育经历",
      },
    }],
    [],
    "form-context-section.example",
  );
  assert.equal(resolved.nodes[0]?.mapping?.canonicalField, "university");
  assert.equal(resolved.nodes[0]?.mapping?.resumePath, "education[0].school");
  assert.equal(resolved.nodes[0]?.mapping?.source, "section");
});

test("resume upload detection can use bounded form context", () => {
  assert.equal(
    isResumeUpload({
      role: "button",
      name: "",
      value: "未选择任何文件",
      actionable: true,
      actions: ["upload_file"],
      form_context: {
        field_signature: "aaaaaaaaaaaaaaaaaaaaaaaa",
        dom_tag: "input",
        input_type: "file",
        section_label: "简历附件",
      },
    }),
    true,
  );
});

test("unlabeled upload controls can be taught safely without pretending every file picker is a resume", () => {
  const first = resolveFormMappings([
    {
      role: "button",
      name: "选择文件",
      value: "未选择任何文件",
      element_id: "element_upload_a",
      actionable: true,
      actions: ["upload_file"],
    },
  ]);
  const mappingId = first.nodes[0]?.mapping_id;
  assert.ok(mappingId);
  assert.equal(first.nodes[0]?.mapping, undefined);

  const taught = resolveFormMappings(
    [
      {
        role: "button",
        name: "选择文件",
        value: "未选择任何文件",
        element_id: "element_upload_b",
        actionable: true,
        actions: ["upload_file"],
      },
    ],
    [{ mapping_id: mappingId, canonicalField: "resume_path" }],
  );
  assert.equal(taught.nodes[0]?.mapping?.canonicalField, "resume_path");
  assert.equal(taught.nodes[0]?.mapping?.resumePath, "attachments.resume_path");
});

test("height and weight use explicit metric profile paths without matching other units", () => {
  assert.equal(matchField("身高（cm）")?.canonicalField, "height_cm");
  assert.equal(matchField("体重（kg）")?.canonicalField, "weight_kg");
  assert.equal(matchField("Height (inches)"), undefined);
  assert.equal(matchField("Weight (lbs)"), undefined);
});
