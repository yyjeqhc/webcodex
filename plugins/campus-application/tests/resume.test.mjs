import assert from "node:assert/strict";
import test from "node:test";
import {
  canonicalProfileFromResume,
  loadResumeProfileFromFile,
  resolveResumeValue,
} from "../dist/resume.js";

function loadExampleProfile() {
  return loadResumeProfileFromFile(
    new URL("../profile.example.json", import.meta.url),
  );
}

test("structured resume projects a stable canonical fill view", () => {
  const resume = loadExampleProfile();
  const profile = canonicalProfileFromResume(resume);

  assert.equal(resume.schema_version, 1);
  assert.ok(resume.education.length >= 1);
  assert.ok(resume.experience.length >= 1);
  assert.ok(resume.projects.length >= 1);
  assert.ok(resume.job_preferences.target_roles.length >= 1);
  assert.equal(profile.full_name, "示例候选人");
  assert.equal(profile.university, "Example University");
  assert.equal(profile.degree, "Master");
  assert.equal(profile.graduation_date, "2027-06");
  assert.equal(
    profile.resume_path,
    "plugins/campus-application/fixtures/sample-resume.pdf",
  );
  assert.equal(profile.accept_transfer, "否");
  assert.equal(profile.education_start_date, "2024-09");
  assert.equal(profile.experience_location, "Shanghai");
  assert.equal(profile.project_name, "Campus Apply Lab");
  assert.equal(profile.project_technologies, "Rust, TypeScript, Browser CDP");
  assert.equal(profile.health_status, "健康");
  assert.equal(profile.height_cm, "170");
  assert.equal(profile.weight_kg, "60");
  assert.equal(profile.student_origin, "示例省示例市");
  assert.equal(profile.student_origin_province, "示例省");
  assert.equal(profile.student_origin_city, "示例市");
  assert.equal(profile.current_residence_province, "Shanghai");
  assert.equal(profile.education_city, "Shanghai");
  assert.equal(profile.is_fresh_graduate, "是");
});

test("indexed resume paths resolve repeated structured entries", () => {
  const resume = loadExampleProfile();

  assert.deepEqual(
    resolveResumeValue(resume, "education[1].school"),
    { found: true, value: "Example Institute of Technology" },
  );
  assert.deepEqual(
    resolveResumeValue(resume, "experience[1].company"),
    { found: true, value: "Example Systems Company" },
  );
  assert.deepEqual(
    resolveResumeValue(resume, "projects[1].technologies"),
    { found: true, value: "Rust, CDP, TypeScript" },
  );
  assert.deepEqual(
    resolveResumeValue(resume, "projects[9].name"),
    { found: false, value: "" },
  );
});
