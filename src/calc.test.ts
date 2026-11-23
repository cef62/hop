import { expect, test } from "vitest";
import { calc } from "./calc";

const cases: [string, string | null][] = [
  // precedence and associativity
  ["2+3*4", "14"],
  ["(2+3)*4", "20"],
  ["10-4-3", "3"],
  ["100/10/5", "2"],
  ["2^3^2", "512"],
  ["2*3^2", "18"],
  ["7%3", "1"],
  ["10-7%3", "9"],
  ["2^10", "1024"],
  // unary
  ["-2^2", "-4"],
  ["(-2)^2", "4"],
  ["2^-1", "0.5"],
  ["-3+5", "2"],
  ["2*-3", "-6"],
  ["+2+-+2", "0"],
  ["--2+1", "3"],
  // decimals and rounding
  ["1.5*2", "3"],
  [".5+.25", "0.75"],
  ["0.1+0.2", "0.3"],
  ["1/3", "0.333333333333"],
  ["2/3", "0.666666666667"],
  // integers within 2^53 stay exact, beyond it they round
  ["1234567890123+1", "1234567890124"],
  ["12345678*98764", "1219308541992"],
  ["1732300012345+1000", "1732300013345"], // a timestamp in ms
  // beyond 2^53, or 12+ digits before the point, rounding would change visible digits: exponent form shows it
  ["2^53+1", "9.00719925474e+15"],
  ["9007199254740991+1", "9.00719925474e+15"],
  ["2^64", "1.84467440737e+19"],
  ["-2^60", "-1.15292150461e+18"],
  ["123456789012.5*1", "1.23456789013e+11"],
  ["12345678901.5*1", "12345678901.5"],
  ["0-0*5", "0"],
  ["-0*1", "0"],
  // exponent form for huge and tiny results
  ["10^21", "1e+21"],
  ["10^20", "1e+20"],
  ["2^100", "1.26765060023e+30"],
  ["1/10^7", "1e-7"],
  // exponent literals: a shown result can be fed back in
  ["1.84467440737e+19*1", "1.84467440737e+19"],
  ["100-1e+2", "0"],
  ["2e3+1", "2001"],
  ["1e-7*10", "0.000001"],
  ["1E3+1", "1001"],
  ["1e+21*1", "1e+21"],
  [".5e2+0", "50"],
  ["1e-400*1", "0"], // underflows to 0, a finite result
  ["-1e+21*1", "-1e+21"],
  ["1e5", null], // a bare literal has no operator: still an app search
  ["1e", null],
  ["1e+", null],
  ["e+5+1", null],
  ["5.e2+0", null], // "5." is no number, with or without an exponent
  ["1e400*1", null], // Infinity
  ["1e5e5+1", null],
  ["1e 5+1", null],
  ["1e+2.5+1", null],
  // whitespace and aliases
  [" 2 + 3 * 4 ", "14"],
  ["6×7", "42"],
  ["9÷4", "2.25"],
  // no row
  ["1/0", null],
  ["0/0", null],
  ["5%0", null],
  ["10^400", null],
  ["2+", null],
  ["2*", null],
  ["(2+3", null],
  ["2+3)", null],
  ["()", null],
  ["2+x", null],
  ["safari", null],
  ["1password", null],
  ["2024", null],
  ["-5", null],
  ["(3)", null],
  ["1.2.3+1", null],
  ["1 2+3", null],
  ["1.+2", null],
  [".+2", null],
  ["2**3", null],
  ["1,5+1", null],
  ["", null],
  ["   ", null],
  [`1${"+1".repeat(99)}`, "100"], // 199 characters
  [`11${"+1".repeat(99)}`, "110"], // 200 characters
  [`1${"+1".repeat(100)}`, null], // 201 characters
  [`${"(".repeat(98)}1+1${")".repeat(98)}`, "2"], // deep nesting within the cap
];

test.each(cases)("calc(%j) = %j", (query, want) => {
  expect(calc(query)).toBe(want);
});

test("any result, fed back in, gives itself again (every exponent shape)", () => {
  let seed = 12345;
  const rnd = () => (seed = (seed * 1664525 + 1013904223) % 2 ** 32) / 2 ** 32;
  let seen = 0;
  for (let n = 0; n < 1000; n++) {
    const e = Math.floor(rnd() * 80) - 40;
    const a = `${rnd() < 0.5 ? "-" : ""}${(rnd() * 10 ** Math.floor(rnd() * 6)).toFixed(Math.floor(rnd() * 5))}`;
    const r = calc(`${a}*10^${e}+${Math.floor(rnd() * 3)}`);
    if (r === null) continue;
    seen++;
    const back = calc(`${r}*1`);
    // one documented exception: a non-integer from 1e11 up rounds to an integer shown in exponent form ("7.63316e+12",
    // not exact), and typed back it is a safe integer, which calc shows in full: the same number, other digits
    const rounded = r.includes("e") && Number.isSafeInteger(Number(r));
    expect(back, r).toBe(rounded ? String(Number(r)) : r);
    expect(Number(back)).toBe(Number(r));
  }
  expect(seen).toBeGreaterThan(900);
});

test("a non-integer from 1e11 up rounds to an integer in exponent form; typed back it shows in full", () => {
  expect(calc("123456789012.5*1")).toBe("1.23456789013e+11");
  expect(calc("1.23456789013e+11*1")).toBe("123456789013");
});
