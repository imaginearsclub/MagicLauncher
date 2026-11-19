// Material Symbols (Apache-2.0), bundled as inline SVG so nothing loads from the network
// (the launcher's CSP only allows its own files). Add an icon: import it here.
import account from "@material-symbols/svg-400/outlined/account_circle.svg?raw";
import arrowDown from "@material-symbols/svg-400/outlined/keyboard_arrow_down.svg?raw";
import build from "@material-symbols/svg-400/outlined/build.svg?raw";
import castle from "@material-symbols/svg-400/outlined/castle.svg?raw";
import check from "@material-symbols/svg-400/outlined/check.svg?raw";
import close from "@material-symbols/svg-400/outlined/close.svg?raw";
import copy from "@material-symbols/svg-400/outlined/content_copy.svg?raw";
import download from "@material-symbols/svg-400/outlined/download.svg?raw";
import extension from "@material-symbols/svg-400/outlined/extension.svg?raw";
import extensionFill from "@material-symbols/svg-400/outlined/extension-fill.svg?raw";
import folder from "@material-symbols/svg-400/outlined/folder_open.svg?raw";
import group from "@material-symbols/svg-400/outlined/group.svg?raw";
import logout from "@material-symbols/svg-400/outlined/logout.svg?raw";
import personAdd from "@material-symbols/svg-400/outlined/person_add.svg?raw";
import play from "@material-symbols/svg-400/outlined/play_arrow-fill.svg?raw";
import rocket from "@material-symbols/svg-400/outlined/rocket_launch.svg?raw";
import rocketFill from "@material-symbols/svg-400/outlined/rocket_launch-fill.svg?raw";
import settings from "@material-symbols/svg-400/outlined/settings.svg?raw";
import sparkles from "@material-symbols/svg-400/outlined/wand_stars.svg?raw";
import sync from "@material-symbols/svg-400/outlined/sync.svg?raw";
import tune from "@material-symbols/svg-400/outlined/tune.svg?raw";
import verified from "@material-symbols/svg-400/outlined/verified.svg?raw";
import warning from "@material-symbols/svg-400/outlined/warning.svg?raw";

export const ICONS = {
  account,
  arrowDown,
  build,
  castle,
  check,
  close,
  copy,
  download,
  extension,
  extensionFill,
  folder,
  group,
  logout,
  personAdd,
  play,
  rocket,
  rocketFill,
  settings,
  sparkles,
  sync,
  tune,
  verified,
  warning,
} as const;

export type IconName = keyof typeof ICONS;
