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
  ["2^53+1", "9007199254740000"],
  ["0-0*5", "0"],
  ["-0*1", "0"],
  // exponent form for huge and tiny results
  ["10^21", "1e+21"],
  ["10^20", "100000000000000000000"],
  ["2^100", "1.26765060023e+30"],
  ["1/10^7", "1e-7"],
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
