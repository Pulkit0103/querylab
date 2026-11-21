import {
  Activity,
  Bookmark,
  Database,
  Gauge,
  History,
  LayoutDashboard,
  Network,
  Settings,
  SquareTerminal,
  Table2,
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
  { href: "/tables", label: "Table Editor", icon: Table2 },
  { href: "/diagrams", label: "ER Diagrams", icon: Network },
  { href: "/monitoring", label: "Monitoring", icon: Activity },
  { href: "/analyzer", label: "Analyzer", icon: Gauge },
  { href: "/saved", label: "Saved Queries", icon: Bookmark },
  { href: "/history", label: "Query History", icon: History },
  { href: "/settings", label: "Settings", icon: Settings },
];
