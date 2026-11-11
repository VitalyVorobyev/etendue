/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the Rust types (`etendue-scene`, `etendue-cli::calibrate`).
 */

/**
 * `etendue-bank.json`.
 */
export interface BankManifest {
  /**
   * What it is, for people.
   */
  name: string;
  /**
   * Format version, [`BANK_VERSION`].
   */
  version: number;
}
