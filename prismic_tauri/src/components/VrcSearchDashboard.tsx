import React, { useState } from 'react';
import {
  Search,
  RotateCcw,
  Check,
  ChevronDown,
  ExternalLink,
  X,
  Layers,
  ShoppingBag,
  ShieldCheck,
  History,
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
  ChevronUp,
  Monitor,
  Smartphone,
} from 'lucide-react';
import type { SearchFilter, TextSearchMethod } from '../types';
import { VrcRankIcon } from './VrcRankIcon';

interface VrcSearchDashboardProps {
  filter: SearchFilter;
  onChange: (updated: Partial<SearchFilter>) => void;
  onReset: () => void;
  onSearch: () => void;
  isOpen: boolean;
  onToggle: () => void;
  totalResults: number;
  showOldImages: boolean;
  onToggleOldImages: (enabled: boolean) => void;
}

type AccentColor = 'cyan' | 'emerald' | 'purple' | 'amber' | 'teal' | 'rose';

const CHECK_THEMES: Record<
  AccentColor,
  { border: string; bg: string; check: string; glow: string; text: string }
> = {
  cyan: {
    border: 'border-cyan-400',
    bg: 'bg-cyan-950/80',
    check: 'text-cyan-300',
    glow: 'shadow-[0_0_12px_rgba(34,211,238,0.4)]',
    text: 'text-cyan-300',
  },
  emerald: {
    border: 'border-emerald-400',
    bg: 'bg-emerald-950/80',
    check: 'text-emerald-300',
    glow: 'shadow-[0_0_12px_rgba(52,211,153,0.4)]',
    text: 'text-emerald-300',
  },
  purple: {
    border: 'border-purple-400',
    bg: 'bg-purple-950/80',
    check: 'text-purple-300',
    glow: 'shadow-[0_0_12px_rgba(192,132,252,0.4)]',
    text: 'text-purple-300',
  },
  amber: {
    border: 'border-amber-400',
    bg: 'bg-amber-950/80',
    check: 'text-amber-300',
    glow: 'shadow-[0_0_12px_rgba(251,191,36,0.4)]',
    text: 'text-amber-300',
  },
  teal: {
    border: 'border-teal-400',
    bg: 'bg-teal-950/80',
    check: 'text-teal-300',
    glow: 'shadow-[0_0_12px_rgba(45,212,191,0.4)]',
    text: 'text-teal-300',
  },
  rose: {
    border: 'border-rose-400',
    bg: 'bg-rose-950/80',
    check: 'text-rose-300',
    glow: 'shadow-[0_0_12px_rgba(244,63,94,0.4)]',
    text: 'text-rose-300',
  },
};

const StylizedCheckbox: React.FC<{
  checked: boolean;
  onChange: (checked: boolean) => void;
  accent?: AccentColor;
  label?: React.ReactNode;
  icon?: React.ReactNode;
}> = ({ checked, onChange, accent = 'cyan', label, icon }) => {
  const t = CHECK_THEMES[accent];
  return (
    <button
      type="button"
      onClick={() => onChange(!checked)}
      className="group inline-flex items-center gap-2 cursor-pointer select-none transition-all py-1.5 px-2 rounded-xl hover:bg-gray-800/60 active:scale-95 text-left"
    >
      <span
        className={`flex items-center justify-center w-[20px] h-[20px] rounded-lg border-2 transition-all duration-200 shrink-0 ${
          checked
            ? `${t.border} ${t.bg} ${t.glow} scale-105`
            : 'border-gray-700 bg-gray-950/90 group-hover:border-gray-500'
        }`}
      >
        <Check
          strokeWidth={3.5}
          className={`w-3.5 h-3.5 ${t.check} transition-all duration-150 ${
            checked ? 'opacity-100 scale-100' : 'opacity-0 scale-50'
          }`}
        />
      </span>
      {icon}
      {label && (
        <span
          className={`text-xs font-semibold tracking-wide transition-colors ${
            checked ? 'text-gray-100' : 'text-gray-400 group-hover:text-gray-200'
          }`}
        >
          {label}
        </span>
      )}
    </button>
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
  { value: 1, label: 'Excellent', icon: Star, color: 'text-emerald-400', ring: 'ring-emerald-500/40 bg-emerald-950/60' },
  { value: 2, label: 'Good', icon: CircleDot, color: 'text-lime-400', ring: 'ring-lime-500/40 bg-lime-950/60' },
  { value: 3, label: 'Medium', icon: Circle, color: 'text-amber-400', ring: 'ring-amber-500/40 bg-amber-950/60' },
  { value: 4, label: 'Poor', icon: Circle, color: 'text-orange-400', ring: 'ring-orange-500/40 bg-orange-950/60' },
  { value: 5, label: 'Very Poor', icon: AlertTriangle, color: 'text-rose-400', ring: 'ring-rose-500/40 bg-rose-950/60' },
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

export const VrcSearchDashboard: React.FC<VrcSearchDashboardProps> = ({
  filter,
  onChange,
  onReset,
  onSearch,
  isOpen,
  onToggle,
  totalResults,
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

  if (!isOpen) {
    return (
      <div className="bg-[#0b0f19]/90 border-b border-gray-800/80 px-4 py-2 flex items-center justify-between text-xs">
        <div className="flex items-center gap-2">
          <button
            onClick={onToggle}
            className="flex items-center gap-2 px-3.5 py-1.5 rounded-xl bg-cyan-950/50 hover:bg-cyan-900/50 border border-cyan-700/50 text-cyan-300 font-bold transition-all shadow-md shadow-cyan-950/50"
          >
            <Search className="w-3.5 h-3.5" />
            <span>Show Search Dashboard</span>
            <ChevronDown className="w-3.5 h-3.5" />
          </button>

          {/* Quick Active Filters Badges */}
          {(filter.name || filter.author || filter.platform_pc || filter.platform_quest || filter.platform_ios || selectedStylesCount > 0) && (
            <div className="hidden sm:flex items-center gap-1.5 text-[11px] text-gray-400">
              <span className="text-gray-500">Active filters:</span>
              {filter.name && (
                <span className="px-2 py-0.5 rounded-md bg-cyan-950/60 border border-cyan-800/40 text-cyan-300 truncate max-w-[120px]">
                  Name: {filter.name}
                </span>
              )}
              {filter.author && (
                <span className="px-2 py-0.5 rounded-md bg-purple-950/60 border border-purple-800/40 text-purple-300 truncate max-w-[120px]">
                  Author: {filter.author}
                </span>
              )}
              {selectedStylesCount > 0 && (
                <span className="px-2 py-0.5 rounded-md bg-teal-950/60 border border-teal-800/40 text-teal-300">
                  {selectedStylesCount} Styles
                </span>
              )}
            </div>
          )}
        </div>

        <button
          onClick={onReset}
          className="flex items-center gap-1.5 px-3 py-1 rounded-lg text-[11px] text-gray-400 hover:text-cyan-300 hover:bg-gray-800/40 transition-colors"
        >
          <RotateCcw className="w-3 h-3" />
          <span>Reset Filters</span>
        </button>
      </div>
    );
  }

  return (
    <div className="bg-[#0b0f19] border-b border-cyan-900/30 shadow-2xl relative z-20">
      {/* 3-Column VRChat Dashboard Grid */}
      <div className="max-w-[1700px] mx-auto p-4 lg:p-6 grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* ================= COLUMN 1: NAME ================= */}
        <div className="flex flex-col justify-between bg-gray-900/40 border border-gray-800/80 rounded-2xl p-4 shadow-lg">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-extrabold tracking-tight text-white flex items-center gap-2">
                <span>Name</span>
              </h2>

              {/* Name Search Method Dropdown */}
              <div className="relative">
                <button
                  type="button"
                  onClick={() => toggleDropdown('name_method')}
                  className="flex items-center gap-1.5 text-xs font-bold text-cyan-300 bg-cyan-950/60 hover:bg-cyan-900/70 border border-cyan-700/60 rounded-xl px-3 py-1.5 transition-all shadow-sm"
                >
                  <span className="text-[11px] text-gray-400 font-medium">Method:</span>
                  <span>
                    {SEARCH_METHODS.find((m) => m.value === (filter.name_search_method || 'match_all_words'))?.label}
                  </span>
                  <ChevronDown className="w-3.5 h-3.5 text-cyan-400" />
                </button>

                {activeDropdown === 'name_method' && (
                  <div className="absolute right-0 mt-1.5 w-48 bg-gray-900/95 backdrop-blur-md border border-gray-700 rounded-2xl shadow-2xl py-1.5 z-50 animate-in fade-in zoom-in-95 duration-100">
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
                          className={`w-full flex items-center justify-between px-3.5 py-2 text-xs text-left transition-colors ${
                            active ? 'bg-cyan-500/20 text-cyan-300 font-bold' : 'text-gray-300 hover:bg-gray-800'
                          }`}
                        >
                          <span>{method.label}</span>
                          {active && <Check className="w-3.5 h-3.5 text-cyan-400" />}
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>

            {/* Include Description & Tags Stylized Checkbox */}
            <StylizedCheckbox
              checked={filter.name_include_desc_tags !== false}
              onChange={(val) => onChange({ name_include_desc_tags: val, page: 0 })}
              accent="cyan"
              label="Include Description & Tags"
            />

            {/* Input Box with > prefix and X button */}
            <div className="relative flex items-center pt-1">
              <span className="absolute left-3.5 text-cyan-400/70 font-black text-sm select-none">&gt;</span>
              <input
                type="text"
                value={filter.name || ''}
                onChange={(e) => onChange({ name: e.target.value, page: 0 })}
                onKeyDown={(e) => e.key === 'Enter' && onSearch()}
                placeholder="Enter avatar name..."
                className="w-full pl-8 pr-9 py-2.5 text-sm rounded-xl bg-gray-950/90 border border-gray-700/80 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-cyan-500/50 focus:border-cyan-500 transition-all font-medium shadow-inner"
              />
              {filter.name ? (
                <button
                  type="button"
                  onClick={() => onChange({ name: '', page: 0 })}
                  className="absolute right-3 p-1 rounded-lg hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
                >
                  <X className="w-4 h-4" />
                </button>
              ) : null}
            </div>
          </div>

          <div className="mt-4 pt-3 border-t border-gray-800/60 text-[11px] text-gray-500 flex items-center justify-between">
            <span>Matches title, description & style tags</span>
            <span>Press Enter to search</span>
          </div>
        </div>

        {/* ================= COLUMN 2: CENTER GLOBAL FILTERS ================= */}
        <div className="bg-gray-900/40 border border-gray-800/80 rounded-2xl p-4 shadow-lg space-y-4">
          {/* Platform Compatibility Header & Styled Buttons */}
          <div>
            <div className="text-xs font-bold uppercase tracking-wider text-gray-300 mb-2.5 text-center">
              Platform Compatibility
            </div>
            <div className="grid grid-cols-3 gap-2.5">
              {/* PC Platform Button */}
              <button
                type="button"
                onClick={() => onChange({ platform_pc: !filter.platform_pc, page: 0 })}
                className={`flex items-center justify-center gap-2 py-2 px-3 rounded-2xl border-2 text-xs font-extrabold transition-all duration-200 active:scale-95 ${
                  filter.platform_pc
                    ? 'bg-cyan-950/70 border-cyan-400 text-cyan-300 shadow-[0_0_15px_rgba(34,211,238,0.35)]'
                    : 'bg-gray-950/70 border-gray-800 text-gray-400 hover:border-gray-700 hover:text-gray-200'
                }`}
              >
                <Monitor className={`w-3.5 h-3.5 ${filter.platform_pc ? 'text-cyan-400' : 'text-gray-500'}`} />
                <span>PC</span>
              </button>

              {/* Android Platform Button */}
              <button
                type="button"
                onClick={() => onChange({ platform_quest: !filter.platform_quest, page: 0 })}
                className={`flex items-center justify-center gap-2 py-2 px-3 rounded-2xl border-2 text-xs font-extrabold transition-all duration-200 active:scale-95 ${
                  filter.platform_quest
                    ? 'bg-emerald-950/70 border-emerald-400 text-emerald-300 shadow-[0_0_15px_rgba(52,211,153,0.35)]'
                    : 'bg-gray-950/70 border-gray-800 text-gray-400 hover:border-gray-700 hover:text-gray-200'
                }`}
              >
                <Bot className={`w-3.5 h-3.5 ${filter.platform_quest ? 'text-emerald-400' : 'text-gray-500'}`} />
                <span>Android</span>
              </button>

              {/* iOS Platform Button */}
              <button
                type="button"
                onClick={() => onChange({ platform_ios: !filter.platform_ios, page: 0 })}
                className={`flex items-center justify-center gap-2 py-2 px-3 rounded-2xl border-2 text-xs font-extrabold transition-all duration-200 active:scale-95 ${
                  filter.platform_ios
                    ? 'bg-purple-950/70 border-purple-400 text-purple-300 shadow-[0_0_15px_rgba(192,132,252,0.35)]'
                    : 'bg-gray-950/70 border-gray-800 text-gray-400 hover:border-gray-700 hover:text-gray-200'
                }`}
              >
                <Smartphone className={`w-3.5 h-3.5 ${filter.platform_ios ? 'text-purple-400' : 'text-gray-500'}`} />
                <span>iOS</span>
              </button>
            </div>

            {/* Stylized Impostors & Marketplace toggles */}
            <div className="flex items-center justify-around gap-2 pt-3">
              <StylizedCheckbox
                checked={filter.include_impostors}
                onChange={(val) => onChange({ include_impostors: val, page: 0 })}
                accent="teal"
                icon={<Layers className="w-3.5 h-3.5 text-teal-400" />}
                label="Include Impostors"
              />

              <StylizedCheckbox
                checked={filter.include_marketplace !== false}
                onChange={(val) => onChange({ include_marketplace: val, page: 0 })}
                accent="amber"
                icon={<ShoppingBag className="w-3.5 h-3.5 text-amber-400" />}
                label="Include Marketplace"
              />
            </div>
          </div>

          {/* Minimum Performance */}
          <div className="pt-2 border-t border-gray-800/60">
            <div className="text-[11px] font-bold uppercase tracking-wider text-gray-400 mb-2 flex items-center justify-between">
              <span className="flex items-center gap-1.5">
                <ShieldCheck className="w-3.5 h-3.5 text-cyan-400" />
                Minimum Performance
              </span>
            </div>

            <div className="grid grid-cols-3 gap-2">
              {/* PC Perf Dropdown */}
              <div className="relative">
                <button
                  type="button"
                  onClick={() => toggleDropdown('dash_perf_pc')}
                  className="w-full flex items-center justify-between px-2 py-1.5 rounded-xl border border-cyan-700/60 bg-cyan-950/50 hover:bg-cyan-900/60 text-[11px] font-bold text-gray-200 transition-all shadow-sm active:scale-95"
                >
                  <div className="flex items-center gap-1.5 truncate">
                    <VrcRankIcon rank={filter.pc_rating || 5} className="w-4 h-4 shrink-0 drop-shadow-sm" />
                    <span className="truncate">
                      {filter.pc_rating
                        ? PERF_RATINGS.find((r) => r.value === filter.pc_rating)?.label
                        : 'PC: Very Poor'}
                    </span>
                  </div>
                  <ChevronDown className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                </button>
                {activeDropdown === 'dash_perf_pc' && (
                  <div className="absolute left-0 mt-1.5 w-44 bg-gray-900/95 backdrop-blur-md border border-gray-700 rounded-2xl shadow-2xl py-1.5 z-50 animate-in fade-in zoom-in-95 duration-100">
                    {PERF_RATINGS.map((r) => (
                      <button
                        key={r.value}
                        type="button"
                        onClick={() => {
                          onChange({ pc_rating: r.value === 5 ? undefined : r.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className="w-full flex items-center gap-2.5 px-3 py-1.5 text-xs font-semibold text-left hover:bg-gray-800 transition-colors"
                      >
                        <VrcRankIcon rank={r.value} className="w-4 h-4 shrink-0" />
                        <span className="text-gray-200">{r.label}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>

              {/* Quest Perf Dropdown */}
              <div className="relative">
                <button
                  type="button"
                  onClick={() => toggleDropdown('dash_perf_quest')}
                  className="w-full flex items-center justify-between px-2 py-1.5 rounded-xl border border-emerald-700/60 bg-emerald-950/50 hover:bg-emerald-900/60 text-[11px] font-bold text-gray-200 transition-all shadow-sm active:scale-95"
                >
                  <div className="flex items-center gap-1.5 truncate">
                    <VrcRankIcon rank={filter.quest_rating || 5} className="w-4 h-4 shrink-0 drop-shadow-sm" />
                    <span className="truncate">
                      {filter.quest_rating
                        ? PERF_RATINGS.find((r) => r.value === filter.quest_rating)?.label
                        : 'Qst: Very Poor'}
                    </span>
                  </div>
                  <ChevronDown className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                </button>
                {activeDropdown === 'dash_perf_quest' && (
                  <div className="absolute left-0 mt-1.5 w-44 bg-gray-900/95 backdrop-blur-md border border-gray-700 rounded-2xl shadow-2xl py-1.5 z-50 animate-in fade-in zoom-in-95 duration-100">
                    {PERF_RATINGS.map((r) => (
                      <button
                        key={r.value}
                        type="button"
                        onClick={() => {
                          onChange({ quest_rating: r.value === 5 ? undefined : r.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className="w-full flex items-center gap-2.5 px-3 py-1.5 text-xs font-semibold text-left hover:bg-gray-800 transition-colors"
                      >
                        <VrcRankIcon rank={r.value} className="w-4 h-4 shrink-0" />
                        <span className="text-gray-200">{r.label}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>

              {/* iOS Perf Dropdown */}
              <div className="relative">
                <button
                  type="button"
                  onClick={() => toggleDropdown('dash_perf_ios')}
                  className="w-full flex items-center justify-between px-2 py-1.5 rounded-xl border border-purple-700/60 bg-purple-950/50 hover:bg-purple-900/60 text-[11px] font-bold text-gray-200 transition-all shadow-sm active:scale-95"
                >
                  <div className="flex items-center gap-1.5 truncate">
                    <VrcRankIcon rank={filter.ios_rating || 5} className="w-4 h-4 shrink-0 drop-shadow-sm" />
                    <span className="truncate">
                      {filter.ios_rating
                        ? PERF_RATINGS.find((r) => r.value === filter.ios_rating)?.label
                        : 'iOS: Very Poor'}
                    </span>
                  </div>
                  <ChevronDown className="w-3.5 h-3.5 text-purple-400 shrink-0" />
                </button>
                {activeDropdown === 'dash_perf_ios' && (
                  <div className="absolute right-0 mt-1.5 w-44 bg-gray-900/95 backdrop-blur-md border border-gray-700 rounded-2xl shadow-2xl py-1.5 z-50 animate-in fade-in zoom-in-95 duration-100">
                    {PERF_RATINGS.map((r) => (
                      <button
                        key={r.value}
                        type="button"
                        onClick={() => {
                          onChange({ ios_rating: r.value === 5 ? undefined : r.value, page: 0 });
                          setActiveDropdown(null);
                        }}
                        className="w-full flex items-center gap-2.5 px-3 py-1.5 text-xs font-semibold text-left hover:bg-gray-800 transition-colors"
                      >
                        <VrcRankIcon rank={r.value} className="w-4 h-4 shrink-0" />
                        <span className="text-gray-200">{r.label}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Style Filter (Max 2) */}
          <div className="pt-2 border-t border-gray-800/60">
            <div className="flex items-center justify-between text-xs font-bold text-gray-300 mb-2">
              <span>Style Filter (Max 2)</span>
              <span className="text-[11px] text-cyan-400 font-extrabold px-2 py-0.5 rounded-full bg-cyan-950/60 border border-cyan-800/50">
                {selectedStylesCount}/2
              </span>
            </div>

            <div className="flex flex-wrap gap-1.5">
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
                    className={`flex items-center gap-1.5 px-2.5 py-1 rounded-xl border text-[11px] font-semibold transition-all duration-150 active:scale-95 ${
                      selected
                        ? 'bg-cyan-950/90 border-cyan-400 text-cyan-200 shadow-[0_0_10px_rgba(34,211,238,0.3)] scale-105'
                        : disabled
                        ? 'bg-gray-950/20 border-gray-800/40 text-gray-600 cursor-not-allowed'
                        : 'bg-gray-950/70 border-gray-800 text-gray-400 hover:text-gray-100 hover:border-gray-600 hover:bg-gray-900'
                    }`}
                  >
                    <Icon className="w-3.5 h-3.5" />
                    <span>{style.label}</span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Allowed Content Warnings */}
          <div className="pt-2 border-t border-gray-800/60">
            <div className="text-xs font-bold text-gray-300 mb-2">Allowed Content Warnings</div>
            <div className="flex flex-wrap gap-1.5">
              {WARNINGS.map((warning) => {
                const isAllowed = (currentWarnings & warning.bit) !== 0;
                const Icon = warning.icon;

                return (
                  <button
                    key={warning.bit}
                    type="button"
                    onClick={() => handleWarningToggle(warning.bit)}
                    className={`flex items-center gap-1.5 px-2.5 py-1 rounded-xl border text-[11px] font-semibold transition-all duration-150 active:scale-95 ${
                      isAllowed
                        ? 'bg-rose-950/50 border-rose-500/50 text-rose-300 shadow-[0_0_10px_rgba(244,63,94,0.2)]'
                        : 'bg-gray-950/50 border-gray-800/60 text-gray-600 line-through opacity-50'
                    }`}
                    title={isAllowed ? 'Allowed (Click to Block)' : 'Blocked (Click to Allow)'}
                  >
                    <Icon className="w-3.5 h-3.5" />
                    <span>{warning.label}</span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Bottom Settings Row: Page Limit & Stylized Historical & Favorites */}
          <div className="pt-2 border-t border-gray-800/60 flex flex-wrap items-center justify-between gap-2 text-xs text-gray-400">
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-semibold">Limit:</span>
              <div className="flex gap-1 bg-gray-950/80 p-0.5 rounded-xl border border-gray-800">
                {RESULT_LIMITS.map((limit) => (
                  <button
                    key={limit}
                    onClick={() => onChange({ page_size: limit, page: 0 })}
                    className={`px-2 py-0.5 rounded-lg text-[10px] font-bold transition-all ${
                      (filter.page_size || 24) === limit
                        ? 'bg-cyan-500 text-black shadow-sm'
                        : 'text-gray-400 hover:text-white'
                    }`}
                  >
                    {limit}
                  </button>
                ))}
              </div>
            </div>

            <div className="flex items-center gap-1">
              <StylizedCheckbox
                checked={filter.favorites_only || false}
                onChange={(val) => onChange({ favorites_only: val, page: 0 })}
                accent="amber"
                icon={<Star className={`w-3.5 h-3.5 ${filter.favorites_only ? 'fill-amber-400 text-amber-400' : 'text-amber-400'}`} />}
                label="Favorites"
              />

              <StylizedCheckbox
                checked={showOldImages}
                onChange={onToggleOldImages}
                accent="purple"
                icon={<History className="w-3.5 h-3.5 text-purple-400" />}
                label="Historical"
              />
            </div>
          </div>
        </div>

        {/* ================= COLUMN 3: AUTHOR ================= */}
        <div className="flex flex-col justify-between bg-gray-900/40 border border-gray-800/80 rounded-2xl p-4 shadow-lg">
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-extrabold tracking-tight text-white flex items-center gap-2">
                <span>Author</span>
              </h2>

              {/* Author Search Method Dropdown */}
              <div className="relative">
                <button
                  type="button"
                  onClick={() => toggleDropdown('author_method')}
                  className="flex items-center gap-1.5 text-xs font-bold text-cyan-300 bg-cyan-950/60 hover:bg-cyan-900/70 border border-cyan-700/60 rounded-xl px-3 py-1.5 transition-all shadow-sm"
                >
                  <span className="text-[11px] text-gray-400 font-medium">Method:</span>
                  <span>
                    {SEARCH_METHODS.find((m) => m.value === (filter.author_search_method || 'match_all_words'))?.label}
                  </span>
                  <ChevronDown className="w-3.5 h-3.5 text-cyan-400" />
                </button>

                {activeDropdown === 'author_method' && (
                  <div className="absolute right-0 mt-1.5 w-48 bg-gray-900/95 backdrop-blur-md border border-gray-700 rounded-2xl shadow-2xl py-1.5 z-50 animate-in fade-in zoom-in-95 duration-100">
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
                          className={`w-full flex items-center justify-between px-3.5 py-2 text-xs text-left transition-colors ${
                            active ? 'bg-cyan-500/20 text-cyan-300 font-bold' : 'text-gray-300 hover:bg-gray-800'
                          }`}
                        >
                          <span>{method.label}</span>
                          {active && <Check className="w-3.5 h-3.5 text-cyan-400" />}
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>

            {/* Avatar Creator FAQ link */}
            <div className="pt-0.5">
              <a
                href="https://docs.vrchat.com/docs/avatar-creation-faq"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1.5 text-xs text-cyan-400 hover:text-cyan-300 font-medium transition-colors"
              >
                <span>Avatar Creator FAQ</span>
                <ExternalLink className="w-3.5 h-3.5" />
              </a>
            </div>

            {/* Input Box with > prefix and X button */}
            <div className="relative flex items-center pt-1">
              <span className="absolute left-3.5 text-cyan-400/70 font-black text-sm select-none">&gt;</span>
              <input
                type="text"
                value={filter.author || ''}
                onChange={(e) => onChange({ author: e.target.value, page: 0 })}
                onKeyDown={(e) => e.key === 'Enter' && onSearch()}
                placeholder="Enter author name..."
                className="w-full pl-8 pr-9 py-2.5 text-sm rounded-xl bg-gray-950/90 border border-gray-700/80 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-cyan-500/50 focus:border-cyan-500 transition-all font-medium shadow-inner"
              />
              {filter.author ? (
                <button
                  type="button"
                  onClick={() => onChange({ author: '', page: 0 })}
                  className="absolute right-3 p-1 rounded-lg hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
                >
                  <X className="w-4 h-4" />
                </button>
              ) : null}
            </div>
          </div>

          <div className="mt-4 pt-3 border-t border-gray-800/60 text-[11px] text-gray-500 flex items-center justify-between">
            <span>Filter specifically by author</span>
            <span>Press Enter to search</span>
          </div>
        </div>
      </div>

      {/* Bottom Main Action Bar */}
      <div className="border-t border-gray-800/80 bg-gray-950/90 px-6 py-3 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 text-xs text-gray-400">
          <span className="font-bold text-gray-200 bg-gray-900 border border-gray-800 px-3 py-1.5 rounded-xl">
            {totalResults.toLocaleString()} avatars found
          </span>
          <button
            onClick={onReset}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-gray-400 hover:text-cyan-300 hover:bg-gray-900 border border-transparent hover:border-gray-800 transition-all"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>Reset All</span>
          </button>
        </div>

        {/* Big Search Button + Collapse Button */}
        <div className="flex items-center gap-3">
          <button
            onClick={onSearch}
            className="flex items-center justify-center gap-2 px-9 py-2.5 rounded-xl bg-gradient-to-r from-cyan-600 via-teal-600 to-cyan-500 hover:from-cyan-500 hover:to-teal-400 text-white font-black text-sm tracking-wide uppercase shadow-lg shadow-cyan-600/30 transition-all hover:scale-[1.02] active:scale-[0.98]"
          >
            <Search className="w-4 h-4" />
            <span>Search</span>
          </button>

          <button
            onClick={onToggle}
            className="p-2.5 rounded-xl bg-gray-900 border border-gray-800 hover:bg-gray-800 text-gray-400 hover:text-white transition-all active:scale-95 shadow-sm"
            title="Collapse Dashboard"
          >
            <ChevronUp className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  );
};
