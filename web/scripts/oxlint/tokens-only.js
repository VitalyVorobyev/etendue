// @ts-check
/**
 * oxlint JS plugin `vitavision`, with lab-ui's `tokens-only` rule (from
 * `@vitavision/config-eslint`, ported when the studio moved from ESLint to oxlint): colour comes
 * from the design tokens and nowhere else. Flags, in any string or template literal:
 *
 *   - a raw Tailwind palette utility: `bg-gray-500`, `text-blue-600/50`, `hover:ring-red-400`;
 *   - a hex colour literal: `#1e293b`, `#fff`.
 *
 * Semantic token utilities (`bg-surface`, `text-fg-muted`, `ring-signal`) are what the rule
 * steers to, and are never flagged. `.oxlintrc.json` applies it to the studio's sources.
 */

const PALETTE =
  "slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose";
const UTILITY =
  "bg|text|border|border-[trblxyse]|ring|ring-offset|outline|fill|stroke|from|via|to|decoration|divide|placeholder|shadow|accent|caret";

export const RAW_PALETTE = new RegExp(`(?:^|[\\s:"'\`])(?:${UTILITY})-(?:${PALETTE})-\\d{2,3}(?:/\\d{1,3})?(?=$|[\\s"'\`])`);
export const HEX = /(?:^|[^\w&])#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;

const tokensOnly = {
  meta: {
    type: "problem",
    docs: { description: "Colours come from design tokens: no raw Tailwind palette classes, no hex literals." },
    messages: {
      palette: "Raw Tailwind palette class `{{match}}` — use a semantic token (bg-surface, text-fg-muted, …).",
      hex: "Hex colour literal `{{match}}` — use a design token (a CSS variable or its utility).",
    },
    schema: [],
  },
  /** @param {any} context */
  create(context) {
    /** @param {any} node @param {string} text */
    function check(node, text) {
      const palette = RAW_PALETTE.exec(text);
      if (palette) {
        context.report({ node, messageId: "palette", data: { match: palette[0].trim().replace(/^[:"'`]/, "") } });
        return;
      }
      const hex = HEX.exec(text);
      if (hex) context.report({ node, messageId: "hex", data: { match: hex[0].replace(/^[^#]/, "") } });
    }
    return {
      /** @param {any} node */
      Literal(node) {
        if (typeof node.value === "string") check(node, node.value);
      },
      /** @param {any} node */
      TemplateElement(node) {
        check(node, node.value.cooked ?? node.value.raw);
      },
    };
  },
};

export default { meta: { name: "vitavision" }, rules: { "tokens-only": tokensOnly } };
