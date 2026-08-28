// The pure logic in the UI, checked without a test framework.
//
// There is no test runner in `ui/` and this is not the place to introduce one:
// everything here is a plain function with no DOM, no React and no engine, and
// Node runs the TypeScript directly. The bar for adding to this file is the
// same as the bar for a Rust test — a branch, a loop, or a rule with an edge.
//
//   npm --prefix ui run check:logic
import { urlNotes } from "../src/urlNotes.ts";

let failures = 0;
function is(actual, expected, what) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) {
    console.error(`FAIL  ${what}\n      expected ${e}\n      got      ${a}`);
    failures += 1;
  }
}

const base = "https://example.com";
is(urlNotes(`${base}/blog/post`), [], "a plain path has nothing to report");
is(urlNotes(`${base}/Blog/Post`), ["capitals"], "capitals in the path");
is(urlNotes(`${base}/blog_post`), ["underscores"], "underscores in the path");
is(urlNotes(`${base}/caf%C3%A9`), ["encoded characters"], "percent-encoding");
is(
  urlNotes(`${base}/a/b/c/d/e/f`),
  ["deep path"],
  "six segments is a deep path",
);
is(urlNotes(`${base}/a/b/c/d/e`), [], "five segments is not");
// The host is not the subject: canonicalisation lowercases it, and a capital
// there was never the defect a capital in a path is.
is(urlNotes("https://EXAMPLE.com/blog"), [], "the host is out of scope");
is(
  urlNotes(`${base}/Blog_Post/a/b/c/d/e`),
  ["capitals", "underscores", "deep path"],
  "several at once, in reading order",
);
// The query string is reported by its own column; these still read the path.
is(urlNotes(`${base}/blog?ref=Twitter_x`), [], "the query is another column");

if (failures > 0) {
  console.error(`\n${failures} check${failures === 1 ? "" : "s"} failed.`);
  process.exit(1);
}
console.log("ui logic checks pass.");
