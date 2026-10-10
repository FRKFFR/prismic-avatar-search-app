import React, { useState } from 'react';
import {
  Filter,
  RotateCcw,
  Check,
  ShoppingBag,
  ShieldCheck,
  History,
  X,
  ExternalLink,
  ChevronDown,
  Star,
  CircleDot,
  Circle,
  AlertTriangle,
  User,
  Eye,
  Sparkles,
  Smile,
  PawPrint,
  Dog,
  Box,
  Crown,
  Bot,
  Tv,
  Shirt,
  Flame,
  MessageSquare,
  Sword,
  Skull,
  Ghost,
  Layers,
} from 'lucide-react';
import type { SearchFilter, TextSearchMethod } from '../types';

interface SidebarFiltersProps {
  filter: SearchFilter;
  onChange: (updated: Partial<SearchFilter>) => void;
  onReset: () => void;
  isOpen: boolean;
  onClose?: () => void;
  showOldImages: boolean;
  onToggleOldImages: (enabled: boolean) => void;
}

type Accent = 'cyan' | 'emerald' | 'purple' | 'amber' | 'rose';

const CHECK_COLORS: Record<Accent, { box: string; icon: string; glow: string }> = {
  cyan: {
    box: 'border-cyan-500/70 bg-cyan-950/50',
    icon: 'text-cyan-300',
    glow: 'shadow-[0_0_8px_rgba(34,211,238,0.25)]',
  },
  emerald: {
    box: 'border-emerald-500/70 bg-emerald-950/50',
    icon: 'text-emerald-300',
    glow: 'shadow-[0_0_8px_rgba(52,211,153,0.25)]',
  },
  purple: {
    box: 'border-purple-500/70 bg-purple-950/50',
    icon: 'text-purple-300',
    glow: 'shadow-[0_0_8px_rgba(192,132,252,0.25)]',
  },
  amber: {
    box: 'border-amber-500/70 bg-amber-950/50',
    icon: 'text-amber-300',
    glow: 'shadow-[0_0_8px_rgba(251,191,36,0.25)]',
  },
  rose: {
    box: 'border-rose-500/70 bg-rose-950/50',
    icon: 'text-rose-300',
    glow: 'shadow-[0_0_8px_rgba(244,63,94,0.25)]',
  },
};

const CheckBox: React.FC<{ checked: boolean; accent: Accent }> = ({ checked, accent }) => {
  const c = CHECK_COLORS[accent];
  return (
    <span
      className={`flex items-center justify-center w-[18px] h-[18px] rounded-[5px] border transition-all duration-150 peer-focus-visible:ring-2 peer-focus-visible:ring-cyan-500/40 peer-focus-visible:ring-offset-1 peer-focus-visible:ring-offset-gray-950 ${
        checked
          ? `${c.box} ${c.glow}`
          : 'border-gray-700 bg-gray-950/70 group-hover:border-gray-500'
      }`}
    >
      <Check
        strokeWidth={3}
        className={`w-3 h-3 ${c.icon} transition-all duration-100 ${
          checked ? 'opacity-100 scale-100' : 'opacity-0 scale-50'
        }`}
      />
    </span>
  );
};

const SEARCH_METHODS: { value: TextSearchMethod; label: string }[] = [
  { value: 'match_all_words', label: 'Match All Words' },
  { value: 'match_phrase', label: 'Match Phrase' },
  { value: 'exact_phrase', label: 'Exact Phrase' },
  { value: 'starts_with', label: 'Starts With Phrase' },
  { value: 'ends_with', label: 'Ends With Phrase' },
];

const PERF_RATINGS = [
  { value: 1, label: 'Excellent', icon: Star, color: 'text-emerald-400 bg-emerald-950/60 border-emerald-500/40' },
  { value: 2, label: 'Good', icon: CircleDot, color: 'text-lime-400 bg-lime-950/60 border-lime-500/40' },
  { value: 3, label: 'Medium', icon: Circle, color: 'text-amber-400 bg-amber-950/60 border-amber-500/40' },
  { value: 4, label: 'Poor', icon: Circle, color: 'text-orange-400 bg-orange-950/60 border-orange-500/40' },
  { value: 5, label: 'Very Poor', icon: AlertTriangle, color: 'text-rose-400 bg-rose-950/60 border-rose-500/40' },
];

const STYLES = [
  { bit: 64, label: 'Human', icon: User },
  { bit: 128, label: 'Realistic', icon: Eye },
  { bit: 8, label: 'Anime', icon: Sparkles },
  { bit: 16, label: 'Cartoon', icon: Smile },
  { bit: 2, label: 'Furry', icon: PawPrint },
  { bit: 256, label: 'Animal', icon: Dog },
  { bit: 32, label: 'Objects', icon: Box },
  { bit: 512, label: 'Fantasy', icon: Crown },
  { bit: 4, label: 'Sci-Fi', icon: Bot },
  { bit: 1, label: 'Pop Culture', icon: Tv },
  { bit: 1024, label: 'Fashion', icon: Shirt },
];

const WARNINGS = [
  { bit: 1, label: 'Sexually Suggestive', icon: Flame },
  { bit: 2, label: 'Adult Language & Themes', icon: MessageSquare },
  { bit: 4, label: 'Graphic Violence', icon: Sword },
  { bit: 8, label: 'Excessive Gore', icon: Skull },
  { bit: 16, label: 'Extreme Horror', icon: Ghost },
];

const RESULT_LIMITS = [15, 24, 45, 75, 120];

export const SidebarFilters: React.FC<SidebarFiltersProps> = ({
  filter,
  onChange,
  onReset,
  isOpen,
  showOldImages,
  onToggleOldImages,
}) => {
  const [activeDropdown, setActiveDropdown] = useState<string | null>(null);

  const toggleDropdown = (id: string) => {
    setActiveDropdown((prev) => (prev === id ? null : id));
  };

  const handleStyleToggle = (bit: number) => {
    const current = filter.selected_styles || [];
    if (current.includes(bit)) {
      onChange({ selected_styles: current.filter((b) => b !== bit), page: 0 });
    } else {
      if (current.length < 2) {
        onChange({ selected_styles: [...current, bit], page: 0 });
      }
    }
  };

  const handleWarningToggle = (bit: number) => {
    const currentAllowed = filter.allowed_warnings !== undefined ? filter.allowed_warnings : 31;
    const isAllowed = (currentAllowed & bit) !== 0;
    const nextAllowed = isAllowed ? currentAllowed & ~bit : currentAllowed | bit;
    onChange({ allowed_warnings: nextAllowed, page: 0 });
  };

  const selectedStylesCount = (filter.selected_styles || []).length;
  const currentWarnings = filter.allowed_warnings !== undefined ? filter.allowed_warnings : 31;

  return (
    <aside
      className={`${
        isOpen ? 'block' : 'hidden lg:block'
      } w-full lg:w-80 2xl:w-88 shrink-0 bg-[#0d121f]/95 border-r border-gray-800/80 p-5 overflow-y-auto max-h-[calc(100vh-65px)] custom-scrollbar`}
    >
      {/* Top Header */}
      <div className="flex items-center justify-between pb-4 mb-4 border-b border-gray-800">
        <div className="flex items-center gap-2 text-gray-200 font-semibold text-sm">
          <Filter className="w-4 h-4 text-cyan-400" />
          <span>Filters & Methods</span>
        </div>
        <button
          onClick={onReset}
          className="flex items-center gap-1.5 text-xs text-gray-400 hover:text-cyan-400 transition-colors"
          title="Reset all filters"
        >
          <RotateCcw className="w-3.5 h-3.5" />
          <span>Reset</span>
        </button>
      </div>

      <div className="space-y-6">
        {/* 1. Name Search Section */}
        <div className="space-y-2.5">
          <div className="flex items-center justify-between">
            <label className="text-xs font-bold uppercase tracking-wider text-gray-300">
              Name
            </label>
            {/* Name Search Method Dropdown */}
            <div className="relative">
              <button
                type="button"
                onClick={() => toggleDropdown('name_method')}
                className="flex items-center gap-1 text-[11px] font-semibold text-cyan-400 hover:text-cyan-300 bg-cyan-950/40 hover:bg-cyan-950/70 border border-cyan-800/50 rounded-lg px-2 py-1 transition-colors"
              >
                <span>
                  {SEARCH_METHODS.find((m) => m.value === (filter.name_search_method || 'match_all_words'))?.label}
                </span>
                <ChevronDown className="w-3 h-3" />
              </button>

              {activeDropdown === 'name_method' && (
                <div className="absolute right-0 mt-1 w-44 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl py-1 z-50">
                  {SEARCH_METHODS.map((method) => {
                    const active = (filter.name_search_method || 'match_all_words') === method.value;
                    return (
                      <button
                        key={method.value}
                        type="button"
                        onClick={() => {
                          onChange({ name_search_method: method.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className={`w-full flex items-center justify-between px-3 py-1.5 text-xs text-left transition-colors ${
                          active ? 'bg-cyan-500/20 text-cyan-300 font-semibold' : 'text-gray-300 hover:bg-gray-800'
                        }`}
                      >
                        <span>{method.label}</span>
                        {active && <Check className="w-3 h-3 text-cyan-400" />}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </div>

          {/* Name Input */}
          <div className="relative flex items-center">
            <span className="absolute left-3 text-gray-500 font-bold select-none text-xs">&gt;</span>
            <input
              type="text"
              value={filter.name || ''}
              onChange={(e) => onChange({ name: e.target.value, page: 0 })}
              placeholder="Enter avatar name..."
              className="w-full pl-7 pr-8 py-2 text-xs rounded-xl bg-gray-900/90 border border-gray-800 text-gray-100 placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-cyan-500 focus:border-cyan-500 transition-all"
            />
            {filter.name ? (
              <button
                type="button"
                onClick={() => onChange({ name: '', page: 0 })}
                className="absolute right-2.5 p-0.5 rounded-full hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            ) : null}
          </div>

          {/* Include Description & Tags Checkbox */}
          <label className="group flex items-center gap-2.5 text-xs text-gray-300 hover:text-white cursor-pointer select-none pt-0.5">
            <input
              type="checkbox"
              checked={filter.name_include_desc_tags !== false}
              onChange={(e) => onChange({ name_include_desc_tags: e.target.checked, page: 0 })}
              className="peer sr-only"
            />
            <CheckBox checked={filter.name_include_desc_tags !== false} accent="cyan" />
            <span className="text-[11px] font-medium text-gray-400 group-hover:text-gray-200">
              Include Description & Tags
            </span>
          </label>
        </div>

        {/* 2. Author Search Section */}
        <div className="space-y-2.5 pt-2 border-t border-gray-800/60">
          <div className="flex items-center justify-between">
            <label className="text-xs font-bold uppercase tracking-wider text-gray-300">
              Author
            </label>
            {/* Author Search Method Dropdown */}
            <div className="relative">
              <button
                type="button"
                onClick={() => toggleDropdown('author_method')}
                className="flex items-center gap-1 text-[11px] font-semibold text-cyan-400 hover:text-cyan-300 bg-cyan-950/40 hover:bg-cyan-950/70 border border-cyan-800/50 rounded-lg px-2 py-1 transition-colors"
              >
                <span>
                  {SEARCH_METHODS.find((m) => m.value === (filter.author_search_method || 'match_all_words'))?.label}
                </span>
                <ChevronDown className="w-3 h-3" />
              </button>

              {activeDropdown === 'author_method' && (
                <div className="absolute right-0 mt-1 w-44 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl py-1 z-50">
                  {SEARCH_METHODS.map((method) => {
                    const active = (filter.author_search_method || 'match_all_words') === method.value;
                    return (
                      <button
                        key={method.value}
                        type="button"
                        onClick={() => {
                          onChange({ author_search_method: method.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className={`w-full flex items-center justify-between px-3 py-1.5 text-xs text-left transition-colors ${
                          active ? 'bg-cyan-500/20 text-cyan-300 font-semibold' : 'text-gray-300 hover:bg-gray-800'
                        }`}
                      >
                        <span>{method.label}</span>
                        {active && <Check className="w-3 h-3 text-cyan-400" />}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </div>

          {/* Author Input */}
          <div className="relative flex items-center">
            <span className="absolute left-3 text-gray-500 font-bold select-none text-xs">&gt;</span>
            <input
              type="text"
              value={filter.author || ''}
              onChange={(e) => onChange({ author: e.target.value, page: 0 })}
              placeholder="Enter author name..."
              className="w-full pl-7 pr-8 py-2 text-xs rounded-xl bg-gray-900/90 border border-gray-800 text-gray-100 placeholder-gray-500 focus:outline-none focus:ring-1 focus:ring-cyan-500 focus:border-cyan-500 transition-all"
            />
            {filter.author ? (
              <button
                type="button"
                onClick={() => onChange({ author: '', page: 0 })}
                className="absolute right-2.5 p-0.5 rounded-full hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            ) : null}
          </div>

          {/* Avatar Creator FAQ link */}
          <a
            href="https://docs.vrchat.com/docs/avatar-creation-faq"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1.5 text-[11px] text-cyan-400/80 hover:text-cyan-300 transition-colors font-medium"
          >
            <span>Avatar Creator FAQ</span>
            <ExternalLink className="w-3 h-3" />
          </a>
        </div>

        {/* 3. Platform Compatibility & Impostors */}
        <div className="space-y-3 pt-2 border-t border-gray-800/60">
          <label className="text-xs font-bold uppercase tracking-wider text-gray-300">
            Platform Compatibility
          </label>
          <div className="space-y-2">
            <label className="group flex items-center gap-2.5 text-xs text-gray-300 hover:text-white cursor-pointer select-none">
              <input
                type="checkbox"
                checked={filter.platform_pc}
                onChange={(e) => onChange({ platform_pc: e.target.checked, page: 0 })}
                className="peer sr-only"
              />
              <CheckBox checked={filter.platform_pc} accent="cyan" />
              <span className="inline-block w-2 h-2 rounded-full bg-cyan-400" />
              <span>PC Compatible</span>
            </label>
            <label className="group flex items-center gap-2.5 text-xs text-gray-300 hover:text-white cursor-pointer select-none">
              <input
                type="checkbox"
                checked={filter.platform_quest}
                onChange={(e) => onChange({ platform_quest: e.target.checked, page: 0 })}
                className="peer sr-only"
              />
              <CheckBox checked={filter.platform_quest} accent="emerald" />
              <span className="inline-block w-2 h-2 rounded-full bg-emerald-400" />
              <span>Android / Quest Compatible</span>
            </label>
            <label className="group flex items-center gap-2.5 text-xs text-gray-300 hover:text-white cursor-pointer select-none">
              <input
                type="checkbox"
                checked={filter.platform_ios}
                onChange={(e) => onChange({ platform_ios: e.target.checked, page: 0 })}
                className="peer sr-only"
              />
              <CheckBox checked={filter.platform_ios} accent="purple" />
              <span className="inline-block w-2 h-2 rounded-full bg-purple-400" />
              <span>iOS Compatible</span>
            </label>
          </div>

          <div className="pt-2 border-t border-gray-800/40 space-y-2">
            <label className="group flex items-center justify-between text-xs text-gray-300 hover:text-white cursor-pointer select-none p-1.5 rounded-lg hover:bg-gray-900 transition-colors">
              <span className="flex items-center gap-2">
                <Layers className="w-3.5 h-3.5 text-teal-400" />
                <span>Include Impostors</span>
              </span>
              <input
                type="checkbox"
                checked={filter.include_impostors}
                onChange={(e) => onChange({ include_impostors: e.target.checked, page: 0 })}
                className="peer sr-only"
              />
              <CheckBox checked={filter.include_impostors} accent="emerald" />
            </label>

            <label className="group flex items-center justify-between text-xs text-gray-300 hover:text-white cursor-pointer select-none p-1.5 rounded-lg hover:bg-gray-900 transition-colors">
              <span className="flex items-center gap-2">
                <ShoppingBag className="w-3.5 h-3.5 text-amber-400" />
                <span>Include Marketplace</span>
              </span>
              <input
                type="checkbox"
                checked={filter.include_marketplace !== false}
                onChange={(e) => onChange({ include_marketplace: e.target.checked, page: 0 })}
                className="peer sr-only"
              />
              <CheckBox checked={filter.include_marketplace !== false} accent="amber" />
            </label>
          </div>
        </div>

        {/* 4. Minimum Performance Dropdowns */}
        <div className="space-y-3 pt-2 border-t border-gray-800/60">
          <div className="flex items-center justify-between">
            <label className="text-xs font-bold uppercase tracking-wider text-gray-300 flex items-center gap-1.5">
              <ShieldCheck className="w-3.5 h-3.5 text-cyan-400" />
              <span>Minimum Performance</span>
            </label>
          </div>

          <div className="grid grid-cols-3 gap-2">
            {/* PC Performance */}
            <div className="relative">
              <span className="text-[10px] font-semibold text-cyan-400 block mb-1">PC</span>
              <button
                type="button"
                onClick={() => toggleDropdown('perf_pc')}
                className="w-full flex items-center justify-between px-2 py-1.5 rounded-xl border border-cyan-800/50 bg-cyan-950/40 text-[11px] font-bold text-gray-200 hover:bg-cyan-950/70 transition-colors"
              >
                <span className="truncate">
                  {filter.pc_rating
                    ? PERF_RATINGS.find((r) => r.value === filter.pc_rating)?.label
                    : 'Very Poor'}
                </span>
                <ChevronDown className="w-3 h-3 text-cyan-400 shrink-0" />
              </button>

              {activeDropdown === 'perf_pc' && (
                <div className="absolute left-0 mt-1 w-36 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl py-1 z-50">
                  {PERF_RATINGS.map((r) => {
                    const Icon = r.icon;
                    return (
                      <button
                        key={r.value}
                        type="button"
                        onClick={() => {
                          onChange({ pc_rating: r.value === 5 ? undefined : r.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-left hover:bg-gray-800 transition-colors"
                      >
                        <Icon className="w-3 h-3 shrink-0" />
                        <span className="truncate">{r.label}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Android / Quest Performance */}
            <div className="relative">
              <span className="text-[10px] font-semibold text-emerald-400 block mb-1">Android</span>
              <button
                type="button"
                onClick={() => toggleDropdown('perf_quest')}
                className="w-full flex items-center justify-between px-2 py-1.5 rounded-xl border border-emerald-800/50 bg-emerald-950/40 text-[11px] font-bold text-gray-200 hover:bg-emerald-950/70 transition-colors"
              >
                <span className="truncate">
                  {filter.quest_rating
                    ? PERF_RATINGS.find((r) => r.value === filter.quest_rating)?.label
                    : 'Very Poor'}
                </span>
                <ChevronDown className="w-3 h-3 text-emerald-400 shrink-0" />
              </button>

              {activeDropdown === 'perf_quest' && (
                <div className="absolute left-0 mt-1 w-36 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl py-1 z-50">
                  {PERF_RATINGS.map((r) => {
                    const Icon = r.icon;
                    return (
                      <button
                        key={r.value}
                        type="button"
                        onClick={() => {
                          onChange({ quest_rating: r.value === 5 ? undefined : r.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-left hover:bg-gray-800 transition-colors"
                      >
                        <Icon className="w-3 h-3 shrink-0" />
                        <span className="truncate">{r.label}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            {/* iOS Performance */}
            <div className="relative">
              <span className="text-[10px] font-semibold text-purple-400 block mb-1">iOS</span>
              <button
                type="button"
                onClick={() => toggleDropdown('perf_ios')}
                className="w-full flex items-center justify-between px-2 py-1.5 rounded-xl border border-purple-800/50 bg-purple-950/40 text-[11px] font-bold text-gray-200 hover:bg-purple-950/70 transition-colors"
              >
                <span className="truncate">
                  {filter.ios_rating
                    ? PERF_RATINGS.find((r) => r.value === filter.ios_rating)?.label
                    : 'Very Poor'}
                </span>
                <ChevronDown className="w-3 h-3 text-purple-400 shrink-0" />
              </button>

              {activeDropdown === 'perf_ios' && (
                <div className="absolute right-0 mt-1 w-36 bg-gray-900 border border-gray-700 rounded-xl shadow-2xl py-1 z-50">
                  {PERF_RATINGS.map((r) => {
                    const Icon = r.icon;
                    return (
                      <button
                        key={r.value}
                        type="button"
                        onClick={() => {
                          onChange({ ios_rating: r.value === 5 ? undefined : r.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className="w-full flex items-center gap-1.5 px-2.5 py-1.5 text-[11px] font-medium text-left hover:bg-gray-800 transition-colors"
                      >
                        <Icon className="w-3 h-3 shrink-0" />
                        <span className="truncate">{r.label}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        </div>

        {/* 5. Style Filter (Max 2) */}
        <div className="space-y-2.5 pt-2 border-t border-gray-800/60">
          <div className="flex items-center justify-between">
            <label className="text-xs font-bold uppercase tracking-wider text-gray-300">
              Style Filter (Max 2)
            </label>
            <span className="text-[11px] font-semibold text-cyan-400">
              {selectedStylesCount}/2
            </span>
          </div>

          <div className="grid grid-cols-2 gap-1.5">
            {STYLES.map((style) => {
              const selected = (filter.selected_styles || []).includes(style.bit);
              const disabled = !selected && selectedStylesCount >= 2;
              const Icon = style.icon;

              return (
                <button
                  key={style.bit}
                  type="button"
                  disabled={disabled}
                  onClick={() => handleStyleToggle(style.bit)}
                  className={`flex items-center gap-2 px-2.5 py-1.5 rounded-xl border text-[11px] font-medium transition-all ${
                    selected
                      ? 'bg-cyan-950/80 border-cyan-500/70 text-cyan-300 shadow-[0_0_10px_rgba(34,211,238,0.2)]'
                      : disabled
                      ? 'bg-gray-950/30 border-gray-800/40 text-gray-600 cursor-not-allowed'
                      : 'bg-gray-900/60 border-gray-800 text-gray-400 hover:bg-gray-800/70 hover:text-gray-200'
                  }`}
                >
                  <Icon className="w-3.5 h-3.5 shrink-0" />
                  <span className="truncate">{style.label}</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* 6. Allowed Content Warnings */}
        <div className="space-y-2.5 pt-2 border-t border-gray-800/60">
          <div className="flex items-center justify-between">
            <label className="text-xs font-bold uppercase tracking-wider text-gray-300">
              Allowed Content Warnings
            </label>
          </div>

          <div className="space-y-1.5">
            {WARNINGS.map((warning) => {
              const isAllowed = (currentWarnings & warning.bit) !== 0;
              const Icon = warning.icon;

              return (
                <button
                  key={warning.bit}
                  type="button"
                  onClick={() => handleWarningToggle(warning.bit)}
                  className={`w-full flex items-center justify-between px-3 py-1.5 rounded-xl border text-xs font-medium transition-all ${
                    isAllowed
                      ? 'bg-rose-950/30 border-rose-500/40 text-rose-300'
                      : 'bg-gray-950/40 border-gray-800 text-gray-500 line-through opacity-60'
                  }`}
                >
                  <span className="flex items-center gap-2">
                    <Icon className="w-3.5 h-3.5 text-rose-400" />
                    <span>{warning.label}</span>
                  </span>
                  <span className="text-[10px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded bg-black/40">
                    {isAllowed ? 'Allowed' : 'Blocked'}
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* 7. Result Limit Per Page */}
        <div className="space-y-2.5 pt-2 border-t border-gray-800/60">
          <label className="text-xs font-bold uppercase tracking-wider text-gray-300">
            Result Limit Per Page
          </label>
          <div className="grid grid-cols-5 gap-1.5">
            {RESULT_LIMITS.map((limit) => {
              const active = (filter.page_size || 24) === limit;
              return (
                <button
                  key={limit}
                  type="button"
                  onClick={() => onChange({ page_size: limit, page: 0 })}
                  className={`py-1.5 rounded-xl border text-xs font-bold transition-all ${
                    active
                      ? 'bg-cyan-600 text-white border-cyan-400 shadow-md shadow-cyan-600/30'
                      : 'bg-gray-900 border-gray-800 text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                  }`}
                >
                  {limit}
                </button>
              );
            })}
          </div>
        </div>

        {/* 8. Display Preferences */}
        <div className="pt-2 border-t border-gray-800/60 space-y-2">
          <label className="group flex items-center justify-between text-xs text-gray-300 hover:text-white cursor-pointer select-none p-1.5 rounded-lg hover:bg-gray-900 transition-colors">
            <span className="flex items-center gap-2">
              <History className="w-4 h-4 text-purple-400" />
              <span>Show Historical Images</span>
            </span>
            <input
              type="checkbox"
              checked={showOldImages}
              onChange={(e) => onToggleOldImages(e.target.checked)}
              className="peer sr-only"
            />
            <CheckBox checked={showOldImages} accent="purple" />
          </label>
        </div>
      </div>
    </aside>
  );
};
