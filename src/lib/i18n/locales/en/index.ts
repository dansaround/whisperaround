/**
 * The English dictionary, assembled from one file per area. Its shape is the
 * contract every other locale must match (see `Dict` in `../../index.svelte.ts`).
 */
import common from "./common";
import pill from "./pill";
import welcome from "./welcome";
import shell from "./shell";
import home from "./home";
import modes from "./modes";
import vocabulary from "./vocabulary";
import configuration from "./configuration";
import sound from "./sound";
import models from "./models";
import history from "./history";
import meetings from "./meetings";

export default { common, pill, welcome, shell, home, modes, vocabulary, configuration, sound, models, history, meetings };
