declare module "virtual:etendue-examples" {
  /** An example scene of the repository (`examples/<id>/`). */
  export interface Example {
    id: string;
    description: string;
    /** Repository-relative path of the scene. */
    scene: string;
    /** Repository-relative path of the scenario, if the example has one. */
    scenario: string | null;
  }
  const examples: Example[];
  export default examples;
}
