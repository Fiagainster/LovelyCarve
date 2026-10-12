/** Where a dropdown list goes relative to its trigger, in viewport pixels. */
export interface DropdownPlacement {
  top: number;
  left: number;
  /** Height the list may take before it scrolls. */
  maxHeight: number;
  /** The list opens upward because there is more room above the trigger. */
  above: boolean;
}

interface TriggerBox {
  top: number;
  bottom: number;
  left: number;
}

interface Size {
  width: number;
  height: number;
}

const GAP = 4;
const VIEWPORT_MARGIN = 8;
export const DROPDOWN_MAX_HEIGHT = 320;

/** Opens below the trigger unless the list only fits above (or above has more
 *  room), and keeps it inside the viewport horizontally. */
export function placeDropdown(trigger: TriggerBox, list: Size, viewport: Size): DropdownPlacement {
  const roomBelow = viewport.height - trigger.bottom - GAP - VIEWPORT_MARGIN;
  const roomAbove = trigger.top - GAP - VIEWPORT_MARGIN;
  const wanted = Math.min(list.height, DROPDOWN_MAX_HEIGHT);
  const above = wanted > roomBelow && roomAbove > roomBelow;
  const maxHeight = Math.max(0, Math.min(DROPDOWN_MAX_HEIGHT, above ? roomAbove : roomBelow));
  const height = Math.min(wanted, maxHeight);
  return {
    top: above ? trigger.top - GAP - height : trigger.bottom + GAP,
    left: Math.max(VIEWPORT_MARGIN, Math.min(trigger.left, viewport.width - VIEWPORT_MARGIN - list.width)),
    maxHeight,
    above,
  };
}
