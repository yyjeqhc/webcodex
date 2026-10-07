import assert from "node:assert/strict";
import test from "node:test";
import { loadResumeProfileFromFile } from "../dist/resume.js";
import { resolveChoicePath } from "../dist/location-values.js";
import { canonicalDate, dateForControl } from "../dist/widget-values.js";
import { resolveFormMappings } from "../dist/form-cache.js";
import { createFillBatch, reconcileFill } from "../dist/fill-batch.js";

const loadProfile = () => loadResumeProfileFromFile(new URL("../profile.example.json", import.meta.url));
const cascader = { role: "combobox", name: "籍贯", value: "", actions: ["select_choice"], actionable: true,
  element_id: "cascader", form_context: { field_signature: "cascader", dom_tag: "input", component_hint: "cascader" } };
const dateNode = { role: "textbox", name: "毕业时间", value: "", actions: ["set_date"], actionable: true, read_only: true,
  element_id: "date", form_context: { field_signature: "date", dom_tag: "input", input_type: "text", component_hint: "date-picker" } };
const scope = { client_id: "fixture", browser_id: "browser", page_id: "page", url: "https://fixture.invalid/location-date", snapshot_generation: 1 };
const complete = count => ({ execution_state: "completed", requested_count: count, completed_count: count,
  remaining_count: 0, stability: { stable: true } });

test("hierarchical paths keep native place, household and student origin independent, including districts", () => {
  const resume = loadProfile();
  for (const [field, label] of [["native_place", "籍贯"], ["household_registration", "户籍"], ["student_origin", "生源"]]) {
    resume.personal[field] = "";
    resume.personal[field + "_province"] = label + "省";
    resume.personal[field + "_city"] = label + "市";
    resume.personal[field + "_district"] = label + "区";
    const result = resolveChoicePath(resume, field, "personal." + field, "", cascader);
    assert.deepEqual(result.path, [label + "省", label + "市", label + "区"]);
  }
  const city = resolveChoicePath(resume, "native_place_city", "personal.native_place_city", "籍贯市",
    { ...cascader, form_context: { ...cascader.form_context, component_hint: "select" } });
  assert.deepEqual(city.path, ["籍贯市"]);
  assert.deepEqual(resolveChoicePath(resume, "native_place", "personal.native_place", "", cascader,
    "Taught Province / Taught City / Taught District").path, ["Taught Province", "Taught City", "Taught District"]);
});
test("location paths require complete ordered facts and never turn preference alternatives into hierarchy", () => {
  const resume = loadProfile();
  resume.personal.native_place_city = "";
  resume.personal.native_place_district = "孤立区";
  assert.match(resolveChoicePath(resume, "native_place", "personal.native_place", "", cascader).reason, /incomplete/);
  assert.match(resolveChoicePath(resume, "preferred_locations", "job_preferences.preferred_locations", "Shanghai, Hangzhou", cascader).reason, /alternatives/);
  assert.deepEqual(resolveChoicePath(resume, "preferred_locations", "job_preferences.preferred_locations[1]", "Hangzhou", cascader).path, ["Hangzhou"]);
  resume.job_preferences.preferred_locations = ["Province / City"];
  assert.deepEqual(resolveChoicePath(resume, "preferred_locations", "job_preferences.preferred_locations", "Province / City", cascader).path, ["Province", "City"]);
  assert.equal(resolveChoicePath(resume, "native_place", "personal.native_place", "", cascader, "a/b/c/d/e").path, undefined);
  assert.deepEqual(resolveChoicePath(resume, "degree", "education[0].degree", "R&D / Operations",
    { ...cascader, form_context: { ...cascader.form_context, component_hint: "select" } }).path, ["R&D / Operations"]);
});
test("bare province/city/district controls use their own form group instead of contact defaults", () => {
  const groups = [["籍贯", "native_place"], ["户籍", "household_registration"], ["生源地", "student_origin"]];
  const nodes = groups.flatMap(([label], group) => ["省份", "城市", "区县"].map((name, axis) => ({
    role: "combobox", name, group_label: label, group_id: "group-" + group, actionable: true,
    actions: ["select_option"], element_id: "field-" + group + "-" + axis,
    form_context: { field_signature: "sig-" + group + "-" + axis, dom_tag: "select", group_index: axis, group_size: 3 },
  })));
  const resolved = resolveFormMappings(nodes, [], "separate-location-groups");
  assert.deepEqual(resolved.nodes.map(item => item.mapping.resumePath),
    groups.flatMap(([, field]) => ["province", "city", "district"].map(axis => "personal." + field + "_" + axis)));
});
test("custom date canonicalization keeps precision, validates calendar dates and never invents a day", () => {
  for (const [input, expected] of [["2027-06", "2027-06"], ["2027/6/3", "2027-06-03"],
    ["预计 2027年6月", "2027-06"], ["2024年2月29日", "2024-02-29"], ["Expected in 2027.06", "2027-06"]]) {
    assert.equal(canonicalDate(input), expected);
  }
  for (const input of ["2027-02-29", "1900-02-29", "2027-13", "2027-00-01", "0000-06", "06/07/2027", "至今", "2027-06-31", "2027-06-01T12:30"]) {
    assert.equal(canonicalDate(input), undefined);
  }
  assert.equal(dateForControl("2027-06", { ...dateNode, form_context: { ...dateNode.form_context, input_type: "date" } }), undefined);
  assert.equal(dateForControl("2027-06-30", { ...dateNode, form_context: { ...dateNode.form_context, input_type: "month" } }), "2027-06");
  assert.equal(dateForControl("2027-06-30", { ...dateNode, name: "出生年月",
    form_context: { ...dateNode.form_context, input_type: "date" } }), "2027-06-30");
});
test("admitted custom date is one operation with canonical readback; malformed/unadmitted targets are not filled", () => {
  const action = { kind: "set_date", element_id: dateNode.element_id, label: dateNode.name, value: "2027-06-30", confidence: 1 };
  const plan = createFillBatch(scope, [dateNode], [action]);
  assert.deepEqual(plan.batch.operations, [{ action: "set_date", element_id: "date", value: "2027-06-30" }]);
  assert.deepEqual(reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 },
    [{ ...dateNode, value: "2027年6月30日", element_id: "fresh-date" }], complete(1)), { confirmed: 1, needs_attention: [] });
  assert.equal(createFillBatch(scope, [dateNode], [{ ...action, value: "2027/6/30" }]).batch, undefined);
  assert.equal(createFillBatch(scope, [{ ...dateNode, actions: ["click"] }], [action]).batch, undefined);
});
