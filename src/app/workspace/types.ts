
export interface NavItem {
  id: string;
  icon: string;
  label: string;
  badge: number | null;
  badgeType: string;
}

/** A category in the workspace rail and its second-level pages. */
export interface NavGroup {
  groupId: string;
  groupLabel: string;
  groupIcon: string;   // Lucide icon name
  groupColor: string;  // CSS color for accent
  items: NavItem[];
}
