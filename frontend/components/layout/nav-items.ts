import {
  Bookmark,
  Database,
  History,
  LayoutDashboard,
  Settings,
  SquareTerminal,
  type LucideIcon,
} from "lucide-react";

export interface NavItem {
  href: string;
  label: string;
  icon: LucideIcon;
}

export const navItems: NavItem[] = [
  { href: "/dashboard", label: "Dashboard", icon: LayoutDashboard },
  { href: "/editor", label: "SQL Editor", icon: SquareTerminal },
  { href: "/explorer", label: "Database Explorer", icon: Database },
  { href: "/saved", label: "Saved Queries", icon: Bookmark },
  { href: "/history", label: "Query History", icon: History },
  { href: "/settings", label: "Settings", icon: Settings },
];
