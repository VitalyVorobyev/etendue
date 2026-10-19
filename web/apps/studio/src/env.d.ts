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
