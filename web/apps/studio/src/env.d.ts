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

declare module "virtual:etendue-robots" {
  /** A robot of the repository's library (`assets/robots/<id>/`). */
  export interface LibraryRobot {
    id: string;
    name: string;
    /** Repository-relative path of its `robot.json`. */
    manifest: string;
    /** Without a redistribution licence: in the library of a development build only. */
    local: boolean;
  }
  const robots: LibraryRobot[];
  export default robots;
}
