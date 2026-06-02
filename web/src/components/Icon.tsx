// Maps semantic icon names (and the macOS SF Symbol names) to Lucide icons, so
// views can use a single <Icon name="…"> matching the desktop app.
import {
  Activity, AlertTriangle, Antenna, ArrowDownToLine, ArrowLeftRight, ArrowUpToLine,
  BadgeCheck, BarChart3, Bell, Box, ChevronRight, CircleDollarSign, CircleUser, Command,
  Copy, CreditCard, Crosshair, Crown, Gauge, Hand, Hash, Hourglass, Inbox, Key, LayoutGrid,
  LineChart, Link as LinkIcon, ListChecks, Lock, Package, Pause, Play, Radio, RefreshCw,
  RotateCw, Search, Send, Server, ShieldCheck, ShoppingCart, Sigma, SlidersHorizontal, Tag,
  Terminal, Trash2, TrendingUp, User, Users, Wifi, Wrench, Zap, CheckCircle2, XOctagon,
  CheckCircle, type LucideIcon,
} from "lucide-react";

const MAP: Record<string, LucideIcon> = {
  // brand / nav
  bolt: Zap, "bolt.fill": Zap, "bolt.horizontal.fill": Zap, "bolt.horizontal": Zap,
  dashboard: LayoutGrid, "square.grid.2x2.fill": LayoutGrid,
  accounts: Users, "person.2.fill": Users, "person.2": Users,
  flips: ArrowLeftRight, "arrow.left.arrow.right": ArrowLeftRight,
  profit: TrendingUp, "chart.line.uptrend.xyaxis": TrendingUp,
  queue: ListChecks, "list.bullet.rectangle.fill": ListChecks, "list.bullet": ListChecks,
  console: Terminal, "terminal.fill": Terminal, terminal: Terminal,
  command: Command,
  connection: Antenna, "antenna.radiowaves.left.and.right": Antenna,
  // semantic
  cart: ShoppingCart, "cart.fill": ShoppingCart,
  seal: BadgeCheck, "checkmark.seal.fill": BadgeCheck, "checkmark.seal": BadgeCheck,
  speedometer: Gauge, gauge: Gauge,
  purse: CreditCard, "creditcard.fill": CreditCard,
  live: Radio, "dot.radiowaves.left.and.right": Radio,
  bars: BarChart3, "chart.bar.fill": BarChart3, "chart.bar.doc.horizontal": BarChart3,
  scope: Crosshair,
  play: Play, "play.fill": Play,
  pause: Pause, "pause.fill": Pause,
  search: Search, magnifyingglass: Search,
  refresh: RotateCw, "arrow.clockwise": RotateCw,
  reconcile: RefreshCw, "arrow.triangle.2.circlepath": RefreshCw,
  trash: Trash2, "trash.fill": Trash2,
  claim: Inbox, "tray.and.arrow.down": Inbox, "tray.and.arrow.down.fill": Inbox,
  bids: Hand, "hand.raised.fill": Hand, "hand.raised": Hand,
  sell: Package, "shippingbox.fill": Package,
  notif: Bell, "bell.fill": Bell,
  activity: Activity, "waveform.path.ecg": Activity,
  person: User, "person.fill": User, "person.crop.circle": CircleUser,
  server: Server, "server.rack": Server,
  key: Key, "key.fill": Key,
  lock: Lock, "lock.fill": Lock, "lock.shield.fill": ShieldCheck,
  number: Hash, sum: Sigma,
  "arrow.down.to.line": ArrowDownToLine, "arrow.up.to.line": ArrowUpToLine,
  chevron: ChevronRight, "chevron.right": ChevronRight,
  send: Send, "paperplane.fill": Send,
  options: SlidersHorizontal, "slider.horizontal.3": SlidersHorizontal,
  wifi: Wifi,
  tag: Tag, "tag.fill": Tag,
  check: CheckCircle2, "checkmark.circle.fill": CheckCircle2, "checkmark.circle": CheckCircle,
  x: XOctagon, "xmark.octagon.fill": XOctagon,
  alert: AlertTriangle, "exclamationmark.triangle.fill": AlertTriangle,
  crown: Crown, "crown.fill": Crown,
  hourglass: Hourglass,
  wrench: Wrench, "wrench.and.screwdriver.fill": Wrench,
  copy: Copy, "doc.on.doc": Copy,
  dollar: CircleDollarSign, "dollarsign.circle": CircleDollarSign,
  link: LinkIcon,
  line: LineChart, "chart.xyaxis.line": LineChart, "chart.bar": BarChart3,
  cube: Box, "cube.fill": Box,
};

export function Icon({ name, size = 14, strokeWidth = 2.4, style, className }: {
  name: string; size?: number; strokeWidth?: number; style?: React.CSSProperties; className?: string;
}) {
  const Cmp = MAP[name] ?? Box;
  return <Cmp size={size} strokeWidth={strokeWidth} style={style} className={className} />;
}
