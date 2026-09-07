/** App icon contract: semantic names bound to lucide-react glyphs. The chrome
 * set keeps the original 1.7 line weight; the small glyphs that CSS renders
 * below the 24-unit lucide grid (checks, chevrons, search) scale the stroke
 * up by the same ratio so the rendered line weight is unchanged. */
import type { LucideIcon } from "lucide-react";
import {
  ChartColumnIncreasing,
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Download,
  Ellipsis,
  Eye,
  EyeOff,
  GripVertical,
  Minus,
  Pencil,
  Pin,
  Play,
  Plus,
  Search,
  Square,
  Trash2,
  Wifi,
  X,
} from "lucide-react";

interface IconProps {
  size?: number;
}

function icon(Lucide: LucideIcon, size: number, strokeWidth = 1.7) {
  function Glyph({ size: override }: IconProps) {
    return <Lucide size={override ?? size} strokeWidth={strokeWidth} />;
  }
  return Glyph;
}

export const PreviewIcon = icon(Eye, 18);
export const EyeOffIcon = icon(EyeOff, 18);
export const EditIcon = icon(Pencil, 18);
export const TrashIcon = icon(Trash2, 18);
export const PlusIcon = icon(Plus, 20);
export const MinimizeIcon = icon(Minus, 16);
export const MaximizeIcon = icon(Square, 16);
/** The two-square window-restore mark, drawn by lucide's copy glyph. */
export const RestoreIcon = icon(Copy, 16);
export const CloseIcon = icon(X, 16);
export const PinIcon = icon(Pin, 16);
export const GripIcon = icon(GripVertical, 18);
export const MoreIcon = icon(Ellipsis, 18);
export const PlayIcon = icon(Play, 16);
export const UpdateIcon = icon(Download, 16);
export const UsageIcon = icon(ChartColumnIncreasing, 16);
export const ConnectivityIcon = icon(Wifi, 16);

export const ChevronDownIcon = icon(ChevronDown, 16, 2.4);
export const ChevronUpIcon = icon(ChevronUp, 16, 2.4);
export const SearchIcon = icon(Search, 16, 2.4);
export const CheckIcon = icon(Check, 14, 3.6);
export const DashIcon = icon(Minus, 14, 3.6);
