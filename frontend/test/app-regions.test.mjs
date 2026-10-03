import test from "node:test";
import assert from "node:assert/strict";
import { BEGIN, END, replaceSectionBundle } from "../scripts/build-work-result-sections.mjs";

const html = `before\n${BEGIN}\nold bundle\n${END}\nafter`;
test("sections build replaces only the exact region and is reproducible", () => {
  const result = replaceSectionBundle(html, "new bundle\n");
  assert.equal(result, `before\n${BEGIN}\nnew bundle\n${END}\nafter`);
  assert.equal(replaceSectionBundle(result, "new bundle\n"), result);
});
test("sections build rejects absent, duplicated and reversed delimiters", () => {
  for (const malformed of ["", BEGIN, END, `${END}\n${BEGIN}`, html + BEGIN, html + END]) {
    assert.throws(() => replaceSectionBundle(malformed, "next"), /region/);
  }
});
test("sections build cannot inject a closing HTML script or another generated region", () => {
  const result = replaceSectionBundle(html, 'const text = "</ScRiPt><p>untrusted</p>";');
  assert.ok(!/<\/script/i.test(result));
  assert.match(result, /<\\\/script/);
  for (const marker of [BEGIN, END]) assert.throws(() => replaceSectionBundle(html, marker), /reserved region/);
});
