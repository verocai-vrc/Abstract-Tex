// How the window looks on this computer: the size of everything in it (S17.11) and its colours (S17.12).
// Both are kept in the app's preferences. The size is applied by Rust, which zooms the window without the
// page needing a permission for it; the colours by `data-look`, which picks a palette in `app.css`.

import { tell } from './controller.svelte';
import { ipc, type Colours, type TextSize } from './ipc';
import { startScreen } from './start.svelte';

/** The sizes on offer, in percent (`abstract_tex::preferences::TEXT_SIZES`). */
export const TEXT_SIZES: readonly TextSize[] = [100, 115, 130, 150, 175, 200];

/** The choices, in the order they are offered. */
export const COLOURS: readonly Colours[] = ['match', 'light', 'dark', 'contrast-light', 'contrast-dark'];

/** A palette in `app.css`. */
export type Palette = Exclude<Colours, 'match'>;

class Look {
  /** The size in use, which is not always the one that was remembered. */
  textSize = $state<TextSize>(100);
  colours = $state<Colours>('match');
}

export const look = new Look();

/** The window's pixel ratio before it was ever enlarged. */
const ratioAtStart = globalThis.devicePixelRatio || 1;

/**
 * How many times the zoom still has to be counted when something is drawn pixel by pixel. In some
 * webviews `devicePixelRatio` grows with it (measured in WebKitGTK, and known of Chromium, which is
 * Windows' WebView2), in others it stays the screen's own, and a PDF drawn at that is blurred by as much
 * as it is enlarged. Which it is here can be seen from how it has changed since the start.
 */
export function zoomMissingFromPixelRatio(atStart: number, now: number, zoom: number): number {
  const hasGrownWithIt = Math.abs(now / atStart - zoom) < 0.05;
  return hasGrownWithIt ? 1 : zoom;
}

/** Device pixels to a CSS pixel in this window, now. */
export function pixelRatio(): number {
  const now = window.devicePixelRatio || 1;
  return now * zoomMissingFromPixelRatio(ratioAtStart, now, look.textSize / 100);
}

async function apply(size: TextSize): Promise<void> {
  await ipc.applyTextSize(size);
  look.textSize = size;
}

/** The palette for a choice, given what the system asks for. */
export function resolveLook(colours: Colours, prefersDark: boolean, prefersMoreContrast: boolean): Palette {
  if (colours !== 'match') return colours;
  if (prefersMoreContrast) return prefersDark ? 'contrast-dark' : 'contrast-light';
  return prefersDark ? 'dark' : 'light';
}

const PREFERS_DARK = '(prefers-color-scheme: dark)';
const PREFERS_MORE_CONTRAST = '(prefers-contrast: more)';

function paint(): void {
  const [dark, moreContrast] = [PREFERS_DARK, PREFERS_MORE_CONTRAST].map((query) => matchMedia(query).matches);
  document.documentElement.dataset.look = resolveLook(look.colours, dark!, moreContrast!);
}

/** Use these colours from now on, and remember them. */
export function setColours(colours: Colours): Promise<void> {
  look.colours = colours;
  paint();
  changing = changing.then(() => remember({ colours }));
  return changing;
}

/** What is being changed now. The next change waits for it: two that overlap could end the wrong way
 * round, with one size remembered and the other on screen. */
let changing: Promise<void> = Promise.resolve();

/** Make everything `size` percent of its usual size, and remember it. */
export function setTextSize(size: TextSize): Promise<void> {
  changing = changing.then(async () => {
    await remember({ textSize: size });
    await apply(size).catch((error) => tell(String(error)));
  });
  return changing;
}

/** A choice that cannot be remembered is still made, for as long as the window is open. */
async function remember(change: Parameters<typeof ipc.setPreferences>[0]): Promise<void> {
  try {
    startScreen.preferences = await ipc.setPreferences(change);
  } catch (error) {
    tell(String(error));
  }
}

/** The look that was chosen, before the window is first drawn. A config folder that does not answer must
 * not keep anyone out: after `limitMs` the window is drawn, and changes if the answer comes. */
export async function applyRememberedLook(limitMs = 1000): Promise<void> {
  const applied = (async () => {
    const preferences = await ipc.preferences();
    startScreen.preferences = preferences;
    look.colours = preferences.colours;
    paint();
    await apply(preferences.textSize);
  })().catch(() => {}); // the usual look, then
  await Promise.race([applied, new Promise((resolve) => setTimeout(resolve, limitMs))]);
  paint();
  // The system can change its mind, as it does at dusk. `paint` asks it only where it was chosen to.
  for (const query of [PREFERS_DARK, PREFERS_MORE_CONTRAST]) matchMedia(query).addEventListener('change', paint);
}
