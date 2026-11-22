import { expect, test } from "vitest";
import { fold, initials, tier } from "./fuzzy";

test("tiers in spec order", () => {
  expect(["mee", "meeting notes", "team meeting", "my extra east", "ameeba", "mixed tree", "xyz"].map((f) => tier("mee", f))).toEqual([6, 5, 4, 3, 2, 1, 0]);
  expect(tier("", "anything")).toBe(5);
});

test("word initials: vsc finds Visual Studio Code, above a substring and below a word start", () => {
  const vsc = fold("Visual Studio Code");
  expect(initials(vsc)).toBe("vsc");
  expect(tier("vsc", vsc)).toBe(3);
  expect(tier("vs", vsc)).toBe(3);
  expect(tier("code", vsc)).toBe(4);
  expect(tier("vsc", fold("FaceTime"))).toBe(0);
});

test("fold drops diacritics and case", () => {
  expect(fold("Résumé")).toBe("resume");
  expect(tier(fold("cafe"), fold("Café Noir"))).toBe(5);
  expect(tier(fold("impostazioni"), fold("Impostazioni di Sistema"))).toBe(5);
});
