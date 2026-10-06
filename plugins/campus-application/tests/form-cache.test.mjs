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
