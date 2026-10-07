import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  loadPersistentMappingHints,
  mappingSiteIdentity,
  rememberPersistentMappingHints,
} from "../dist/mapping-memory.js";

test("persistent mapping memory is scoped by site and structure", () => {
  const dir = mkdtempSync(join(tmpdir(), "campus-mapping-memory-"));
  const path = join(dir, "mapping-memory.json");
  try {
    assert.equal(mappingSiteIdentity("https://JOB.CHINATELECOM.COM.CN/path"), "job.chinatelecom.com.cn");

    const entries = rememberPersistentMappingHints(
      "https://job.chinatelecom.com.cn/form",
      "sig-a",
      [
        {
          mapping_id: "mapping_ethnicity",
          canonicalField: "ethnicity",
          resumePath: "personal.ethnicity",
          label: "民族",
          choiceValue: "汉族",
        },
      ],
      path,
    );
    assert.equal(entries, 1);

    const same = loadPersistentMappingHints(
      "https://job.chinatelecom.com.cn/other",
      "sig-a",
      path,
    );
    assert.equal(same.hit, true);
    assert.equal(same.hints.length, 1);
    assert.equal(same.hints[0]?.canonicalField, "ethnicity");
    assert.equal(same.hints[0]?.resumePath, "personal.ethnicity");
    assert.equal(same.hints[0]?.choiceValue, "汉族");

    const otherSite = loadPersistentMappingHints(
      "https://example.com/form",
      "sig-a",
      path,
    );
    assert.equal(otherSite.hit, false);

    const otherStructure = loadPersistentMappingHints(
      "https://job.chinatelecom.com.cn/form",
      "sig-b",
      path,
    );
    assert.equal(otherStructure.hit, false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("persistent mapping memory updates an existing mapping id", () => {
  const dir = mkdtempSync(join(tmpdir(), "campus-mapping-memory-"));
  const path = join(dir, "mapping-memory.json");
  try {
    rememberPersistentMappingHints(
      "https://example.com/form",
      "sig",
      [
        {
          mapping_id: "mapping_field",
          canonicalField: "ethnicity",
          resumePath: "personal.ethnicity",
        },
      ],
      path,
    );
    rememberPersistentMappingHints(
      "https://example.com/form",
      "sig",
      [
        {
          mapping_id: "mapping_field",
          canonicalField: "political_status",
          resumePath: "personal.political_status",
        },
      ],
      path,
    );

    const loaded = loadPersistentMappingHints(
      "https://example.com/next",
      "sig",
      path,
    );
    assert.equal(loaded.hints.length, 1);
    assert.equal(loaded.hints[0]?.canonicalField, "political_status");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
