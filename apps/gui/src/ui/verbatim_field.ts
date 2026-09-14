/**
 * Turns off every automatic text transform this page can turn off, on one
 * operator input.
 *
 * E3c defect 32. The composer applied macOS automatic *quote substitution*:
 * setting it to `A"b"c` read back as `A”b"c`, with `U+201D` where the operator
 * typed `U+0022`. That corrupted every `update_plan` payload the native run
 * attempted, twice at the same offset, and Core answered each with its own
 * typed parse refusal — the client had sent JSON that was no longer JSON.
 *
 * The rule this encodes: an operator field that can carry code, JSON, a path,
 * a branch name or a tool argument is **verbatim**. What the operator typed is
 * what Core receives. Autocorrection, capitalisation, smart quotes and
 * autocomplete are conveniences for prose, and each one is a silent edit to a
 * value whose exact bytes are the point — an approval's revised input is
 * compared against Core's own bytes, and a substituted quote fails that
 * comparison without ever saying why.
 *
 * `spellcheck="false"` is the load-bearing one on this host: in WebKit the
 * substitution passes ride the same text-checking pipeline spell checking
 * does, and `autocorrect` alone governs correction rather than substitution.
 * The other three close the neighbouring transforms. All four are attributes
 * the page owns; anything the *host* imposes below the page is not reachable
 * from here, and if E3d still observes a substituted quote with real
 * keystrokes then the remaining setting is the webview's and belongs to the
 * shell, not to this module.
 */
export function verbatimField<T extends HTMLElement>(field: T): T {
  for (const [name, value] of VERBATIM_FIELD_ATTRIBUTES) field.setAttribute(name, value);
  // One marker so the guard test can enumerate every field that claims this
  // rule, rather than a list of selectors that drifts from the code.
  field.dataset.verbatimField = "true";
  return field;
}

/** Every attribute a verbatim field must carry. The guard test reads this. */
export const VERBATIM_FIELD_ATTRIBUTES: ReadonlyArray<readonly [string, string]> = [
  ["autocorrect", "off"],
  ["autocapitalize", "off"],
  ["spellcheck", "false"],
  ["autocomplete", "off"],
];
