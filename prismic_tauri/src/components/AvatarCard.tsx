import React, { useState, useEffect } from 'react';
import {
  ExternalLink,
  Shirt,
  Info,
  Sparkles,
  Check,
  Loader2,
  Images,
  User,
  FileText,
  AlertTriangle,
  PawPrint,
  ShoppingBag,
  Clock,
  Star,
} from 'lucide-react';
import type { AvatarEntry } from '../types';
import { api } from '../api';
import { VrcRankIcon } from './VrcRankIcon';

interface AvatarCardProps {
  avatar: AvatarEntry;
  onSelect: (avatar: AvatarEntry) => void;
  onWear: (avatarId: string) => Promise<void>;
  isLoggedIn: boolean;
  showOldImages: boolean;
  isFavorite?: boolean;
  onToggleFavorite?: (avatar: AvatarEntry) => void;
}

const WindowsIcon: React.FC<{ className?: string }> = ({ className = 'w-4 h-4' }) => (
  <svg className={className} viewBox="0 0 24 24" fill="currentColor">
    <path d="M0 3.449L9.75 2.1v9.451H0m10.949-9.602L24 0v11.401H10.95M0 12.6h9.75v9.451L0 20.699M10.95 12.6H24V24l-12.05-1.699" />
  </svg>
);

const AndroidIcon: React.FC<{ className?: string }> = ({ className = 'w-4 h-4' }) => (
  <svg className={className} viewBox="0 0 24 24" fill="currentColor">
    <path d="M17.523 15.3414c-.5511 0-.9993-.4486-.9993-.9997s.4482-.9993.9993-.9993c.551 0 .9993.4482.9993.9993.0001.5511-.4483.9997-.9993.9997m-11.046 0c-.5511 0-.9993-.4486-.9993-.9997s.4482-.9993.9993-.9993c.5511 0 .9993.4482.9993.9993 0 .5511-.4482.9997-.9993.9997m11.4045-6.02l1.9973-3.4592a.416.416 0 00-.1521-.5676.416.416 0 00-.5676.1521l-2.0223 3.503C15.5902 8.4116 13.8553 8.1 12 8.1s-3.5902.3116-5.1367.8497L4.841 5.4467a.4161.4161 0 00-.5677-.1521.4157.4157 0 00-.1521.5676l1.9973 3.4592C2.6889 11.1867.3432 14.6589 0 18.761h24c-.3432-4.1021-2.6889-7.5743-6.1185-9.4396" />
  </svg>
);

const AppleIcon: React.FC<{ className?: string }> = ({ className = 'w-4 h-4' }) => (
  <svg className={className} viewBox="0 0 24 24" fill="currentColor">
    <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.81-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M15.97 6.84c.64-.78 1.08-1.87.96-2.96-.93.04-2.06.62-2.73 1.4-.59.68-1.1 1.77-.96 2.84 1.04.08 2.09-.5 2.73-1.28z" />
  </svg>
);

const RATING_NAMES: Record<number, string> = {
  1: 'Excellent',
  2: 'Good',
  3: 'Medium',
  4: 'Poor',
  5: 'Very Poor',
  0: 'Unrated',
};

const memoryThumbnailCache = new Map<string, string>();
const memoryImageCountCache = new Map<string, number>();
const memoryLastUpdateCache = new Map<string, string>();

export const AvatarCard: React.FC<AvatarCardProps> = ({
  avatar,
  onSelect,
  onWear,
  isLoggedIn,
  showOldImages,
  isFavorite = false,
  onToggleFavorite,
}) => {
  const countCacheKey = `${avatar.avatar_id}|${showOldImages ? 1 : 0}`;
  const [thumbnailUrl, setThumbnailUrl] = useState<string | null>(() => {
    return memoryThumbnailCache.get(avatar.avatar_id) || null;
  });
  const [loadingThumbnail, setLoadingThumbnail] = useState(false);
  const [imageCount, setImageCount] = useState<number | null>(() => {
    return memoryImageCountCache.get(countCacheKey) ?? null;
  });
  const [lastUpdate, setLastUpdate] = useState<string | null>(() => {
    return memoryLastUpdateCache.get(avatar.avatar_id) ?? null;
  });
  const [isEquipping, setIsEquipping] = useState(false);
  const [equipped, setEquipped] = useState(false);

  useEffect(() => {
    let isMounted = true;
    if (isLoggedIn) {
      if (!thumbnailUrl && !memoryThumbnailCache.has(avatar.avatar_id)) {
        setLoadingThumbnail(true);
        api
          .getAvatarThumbnail(avatar.avatar_id)
          .then((thumb) => {
            if (thumb) {
              memoryThumbnailCache.set(avatar.avatar_id, thumb);
            }
            if (isMounted) {
              setThumbnailUrl(thumb);
              setLoadingThumbnail(false);
            }
          })
          .catch((err) => {
            console.error(`[AvatarCard] Thumbnail error for ${avatar.avatar_id}:`, err);
            if (isMounted) setLoadingThumbnail(false);
          });
      }

      // Fetch avatar details for extra image count if not yet cached
      const cachedCount = memoryImageCountCache.get(countCacheKey);
      if (cachedCount != null) {
        if (imageCount !== cachedCount) {
          setImageCount(cachedCount > 1 ? cachedCount : null);
        }
      } else {
        if (imageCount !== null) setImageCount(null);
        api
          .getAvatarDetails(avatar.avatar_id)
          .then((details) => {
            const count = details.images ? details.images.length : 1;
            memoryImageCountCache.set(countCacheKey, count);
            if (isMounted && count > 1) {
              setImageCount(count);
            }
            if (details.updated_at) {
              memoryLastUpdateCache.set(avatar.avatar_id, details.updated_at);
              if (isMounted) {
                setLastUpdate(details.updated_at);
              }
            }
          })
          .catch((err) => {
            console.error(`[AvatarCard] Details error for ${avatar.avatar_id}:`, err);
          });
      }
    } else {
      setThumbnailUrl(null);
      setLoadingThumbnail(false);
      setImageCount(null);
    }

    return () => {
      isMounted = false;
    };
  }, [avatar.avatar_id, isLoggedIn, showOldImages]);

  const handleWear = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!isLoggedIn) {
      alert('Please log in with your VRChat account to equip avatars directly.');
      return;
    }

    try {
      setIsEquipping(true);
      await onWear(avatar.avatar_id);
      setEquipped(true);
      setTimeout(() => setEquipped(false), 3000);
    } catch (err: any) {
      alert(`Failed to wear avatar: ${err?.message || err}`);
    } finally {
      setIsEquipping(false);
    }
  };

  const handleOpenVrc = (e: React.MouseEvent) => {
    e.stopPropagation();
    api.openInBrowser(`https://vrchat.com/home/avatar/${avatar.avatar_id}`);
  };

  const handleFavoriteClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (onToggleFavorite) {
      onToggleFavorite(avatar);
    }
  };

  const isPc = (avatar.platform & 1) !== 0;
  const isQuest = avatar.quest || (avatar.platform & 2) !== 0;
  const isIos = avatar.ios || (avatar.platform & 4) !== 0;

  return (
    <div
      onClick={() => onSelect(avatar)}
      className="group relative flex flex-col bg-[#111624] hover:bg-[#161c2e] border-2 border-gray-800/90 hover:border-cyan-500/70 rounded-2xl overflow-hidden transition-all duration-200 hover:shadow-2xl hover:shadow-cyan-950/40 cursor-pointer text-left"
    >
      {/* 1. TOP CARD HEADER BAR (Author, Last Update & Actions) */}
      <div className="flex items-center justify-between px-3 py-2 bg-gray-950/95 border-b border-gray-800/80 z-20">
        <div className="flex items-center gap-1.5 min-w-0 pr-2">
          <User className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
          <span className="text-xs font-bold text-gray-200 truncate group-hover:text-cyan-300 transition-colors">
            {avatar.author || 'Unknown Author'}
          </span>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {/* Last Update Date with Clock Icon (Exact VRChat style) */}
          <span className="text-[11px] font-bold text-gray-300 tracking-wider flex items-center gap-1 font-mono select-none">
            <span>{lastUpdate || '2026-04-02'}</span>
            <Clock className="w-3.5 h-3.5 text-gray-400" />
          </span>

          <button
            onClick={handleOpenVrc}
            className="p-1 rounded-lg hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
            title="Open on VRChat website"
          >
            <ExternalLink className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* 2. MAIN 4:3 THUMBNAIL CANVAS WITH VRC-STYLE OVERLAYS */}
      <div className="relative aspect-[4/3] w-full bg-gradient-to-br from-gray-950 via-[#0d121f] to-gray-900 flex items-center justify-center overflow-hidden border-b border-gray-800/60">
        <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-transparent to-black/30 z-10 pointer-events-none" />

        {thumbnailUrl ? (
          <img
            src={thumbnailUrl}
            alt={avatar.name}
            className="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300"
          />
        ) : loadingThumbnail ? (
          <div className="flex flex-col items-center gap-1.5 text-cyan-400/80 animate-pulse">
            <Loader2 className="w-6 h-6 animate-spin" />
            <span className="text-[10px] text-gray-400 font-bold tracking-wider uppercase">Loading preview</span>
          </div>
        ) : (
          <div className="w-14 h-14 rounded-full bg-cyan-950/40 border border-cyan-800/40 flex items-center justify-center text-cyan-400 group-hover:scale-110 group-hover:text-cyan-300 transition-transform duration-300">
            <Sparkles className="w-6 h-6" />
          </div>
        )}

        {/* Top-Right: 3 Big Glowing Circular Platform Badges (Exact VRChat Look) */}
        <div className="absolute top-2.5 right-2.5 z-20 flex items-center gap-1.5">
          {/* Windows / PC Platform Badge */}
          <div
            className={`w-7 h-7 rounded-full flex items-center justify-center transition-all ${
              isPc
                ? 'border-2 border-cyan-400 bg-cyan-950/90 text-cyan-300 shadow-[0_0_12px_rgba(34,211,238,0.7)]'
                : 'border border-gray-800 bg-gray-950/90 text-gray-600 opacity-40'
            }`}
            title={isPc ? 'PC Compatible' : 'Not PC Compatible'}
          >
            <WindowsIcon className="w-3.5 h-3.5" />
          </div>

          {/* Android Platform Badge */}
          <div
            className={`w-7 h-7 rounded-full flex items-center justify-center transition-all ${
              isQuest
                ? 'border-2 border-emerald-400 bg-emerald-950/90 text-emerald-300 shadow-[0_0_12px_rgba(52,211,153,0.7)]'
                : 'border border-gray-800 bg-gray-950/90 text-gray-600 opacity-40'
            }`}
            title={isQuest ? 'Android / Quest Compatible' : 'Not Android Compatible'}
          >
            <AndroidIcon className="w-3.5 h-3.5" />
          </div>

          {/* Apple / iOS Platform Badge */}
          <div
            className={`w-7 h-7 rounded-full flex items-center justify-center transition-all ${
              isIos
                ? 'border-2 border-purple-400 bg-purple-950/90 text-purple-300 shadow-[0_0_12px_rgba(192,132,252,0.7)]'
                : 'border border-gray-800 bg-gray-950/90 text-gray-600 opacity-40'
            }`}
            title={isIos ? 'iOS Compatible' : 'Not iOS Compatible'}
          >
            <AppleIcon className="w-3.5 h-3.5" />
          </div>
        </div>

        {/* Right-Edge: 3 Vertical Performance Rating Badges (VRChat Creator Standards) */}
        <div className="absolute right-2.5 bottom-2.5 z-20 flex flex-col gap-1.5 bg-black/85 backdrop-blur-md px-2 py-1.5 rounded-xl border border-gray-700/70 shadow-2xl">
          <div
            className="flex items-center gap-1.5 text-[9px] font-black"
            title={`PC Performance: ${RATING_NAMES[avatar.pc_rating] || 'Unknown'}`}
          >
            <span className="w-5 text-cyan-400 font-bold">PC</span>
            <VrcRankIcon rank={avatar.pc_rating} className="w-4 h-4 shrink-0 drop-shadow-sm" />
          </div>
          <div
            className="flex items-center gap-1.5 text-[9px] font-black"
            title={`Quest Performance: ${RATING_NAMES[avatar.quest_rating] || 'Unknown'}`}
          >
            <span className="w-5 text-emerald-400 font-bold">Qst</span>
            <VrcRankIcon rank={avatar.quest_rating} className="w-4 h-4 shrink-0 drop-shadow-sm" />
          </div>
          <div
            className="flex items-center gap-1.5 text-[9px] font-black"
            title={`iOS Performance: ${RATING_NAMES[avatar.ios_rating] || 'Unknown'}`}
          >
            <span className="w-5 text-purple-400 font-bold">iOS</span>
            <VrcRankIcon rank={avatar.ios_rating} className="w-4 h-4 shrink-0 drop-shadow-sm" />
          </div>
        </div>

        {/* Top-Left: Multi-Images Count Badge */}
        {imageCount && imageCount > 1 ? (
          <div className="absolute top-2.5 left-2.5 z-20">
            <span className="px-2.5 py-1 rounded-xl text-[10px] font-black uppercase tracking-wider bg-amber-950/90 border-2 border-amber-400 text-amber-300 flex items-center gap-1 shadow-md shadow-amber-950/60">
              <Images className="w-3.5 h-3.5" />
              <span>{imageCount} Images</span>
            </span>
          </div>
        ) : null}

        {/* Left-Edge Badges: Style Pill & Warnings & Marketplace */}
        <div className="absolute left-2.5 bottom-2.5 z-20 flex flex-col gap-1 items-start">
          {avatar.tags && avatar.tags.length > 0 && (
            <span className="px-2 py-0.5 rounded-lg text-[10px] font-bold bg-black/80 border border-gray-700/80 text-gray-200 flex items-center gap-1 shadow-md">
              <PawPrint className="w-3 h-3 text-cyan-400" />
              <span>{avatar.tags[0]}</span>
            </span>
          )}

          {avatar.content_warnings > 0 && (
            <span className="px-2 py-0.5 rounded-lg text-[9px] font-bold bg-rose-950/90 border border-rose-500/70 text-rose-300 flex items-center gap-1 shadow-md">
              <AlertTriangle className="w-3 h-3 text-rose-400" />
              <span>CW</span>
            </span>
          )}

          {avatar.marketplace && (
            <span className="px-2 py-0.5 rounded-lg text-[9px] font-bold bg-amber-950/90 border border-amber-500/70 text-amber-300 flex items-center gap-1 shadow-md">
              <ShoppingBag className="w-3 h-3 text-amber-400" />
              <span>Store</span>
            </span>
          )}

          {isFavorite && (
            <span className="px-2 py-0.5 rounded-lg text-[9px] font-extrabold bg-amber-950/90 border border-amber-400 text-amber-300 flex items-center gap-1 shadow-md shadow-amber-500/20">
              <Star className="w-3 h-3 fill-amber-400 text-amber-400" />
              <span>Favorite</span>
            </span>
          )}
        </div>
      </div>

      {/* 3. BOTTOM AREA: Avatar Title, Description & Action Buttons */}
      <div className="p-3.5 flex flex-col flex-1 justify-between gap-2.5 bg-gray-950/60">
        <div>
          <h3 className="font-extrabold text-sm text-gray-100 group-hover:text-cyan-300 transition-colors line-clamp-1 tracking-tight">
            {avatar.name || 'Untitled Avatar'}
          </h3>

          <p className="text-[11px] text-gray-400 mt-1 line-clamp-1 flex items-center gap-1.5 leading-relaxed">
            <FileText className="w-3 h-3 text-gray-500 shrink-0" />
            <span>{avatar.description ? avatar.description : 'No description provided'}</span>
          </p>
        </div>

        {/* Action Buttons: Wear, Details & Favorite (VRChat In-game layout) */}
        <div className="flex items-center gap-2 pt-2 border-t border-gray-800/80">
          <button
            onClick={handleWear}
            disabled={isEquipping}
            className={`flex-1 flex items-center justify-center gap-1.5 py-1.5 px-3 rounded-xl text-xs font-bold tracking-wide transition-all ${
              equipped
                ? 'bg-emerald-600 text-white shadow-md shadow-emerald-600/30'
                : 'bg-cyan-600/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/40 hover:border-cyan-400'
            }`}
          >
            {isEquipping ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : equipped ? (
              <>
                <Check className="w-3.5 h-3.5" />
                <span>Equipped</span>
              </>
            ) : (
              <>
                <Shirt className="w-3.5 h-3.5" />
                <span>Wear</span>
              </>
            )}
          </button>

          <button
            onClick={handleFavoriteClick}
            className={`p-2 rounded-xl transition-all border shadow-sm flex items-center justify-center active:scale-90 ${
              isFavorite
                ? 'bg-amber-950/80 border-amber-500/80 text-amber-300 shadow-[0_0_12px_rgba(251,191,36,0.35)]'
                : 'bg-gray-800/80 hover:bg-gray-700 text-gray-400 hover:text-amber-300 border-gray-700/60'
            }`}
            title={isFavorite ? 'Remove from Favorites' : 'Add to Favorites'}
          >
            <Star
              className={`w-4 h-4 transition-transform duration-150 ${
                isFavorite ? 'fill-amber-400 text-amber-400 scale-110' : ''
              }`}
            />
          </button>

          <button
            onClick={(e) => {
              e.stopPropagation();
              onSelect(avatar);
            }}
            className="p-1.5 rounded-xl bg-gray-800/80 hover:bg-gray-700 text-gray-400 hover:text-white border border-gray-700/60 transition-colors shadow-sm"
            title="View Details"
          >
            <Info className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  );
};
