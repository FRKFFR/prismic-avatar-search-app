import React, { useEffect, useState } from 'react';
import {
  X,
  ExternalLink,
  Copy,
  Check,
  Shirt,
  Shield,
  Tag,
  Loader2,
  Sparkles,
  ChevronLeft,
  ChevronRight,
  Star,
} from 'lucide-react';
import type { AvatarEntry, AvatarOnlineDetails } from '../types';
import { api } from '../api';
import { useToast } from './Toast';
import { VrcRankIcon } from './VrcRankIcon';

interface AvatarModalProps {
  avatar: AvatarEntry | null;
  onClose: () => void;
  onWear: (avatarId: string) => Promise<void>;
  isLoggedIn: boolean;
  isFavorite?: boolean;
  onToggleFavorite?: (avatar: AvatarEntry) => void;
}

const RATING_NAMES: Record<number, string> = {
  0: 'Unknown',
  1: 'Excellent',
  2: 'Good',
  3: 'Medium',
  4: 'Poor',
  5: 'Very Poor',
};

export const AvatarModal: React.FC<AvatarModalProps> = ({
  avatar,
  onClose,
  onWear,
  isLoggedIn,
  isFavorite = false,
  onToggleFavorite,
}) => {
  const { showToast } = useToast();
  const [onlineDetails, setOnlineDetails] = useState<AvatarOnlineDetails | null>(null);
  const [currentImageIdx, setCurrentImageIdx] = useState(0);
  const [resolvedImages, setResolvedImages] = useState<Record<number, string>>({});
  const [loadingImages, setLoadingImages] = useState<Record<number, boolean>>({});
  const [isLoadingDetails, setIsLoadingDetails] = useState(false);
  const [copied, setCopied] = useState(false);
  const [isEquipping, setIsEquipping] = useState(false);

  // 1. When avatar changes, fetch details
  useEffect(() => {
    if (!avatar) return;

    setOnlineDetails(null);
    setCurrentImageIdx(0);
    setResolvedImages({});
    setLoadingImages({});
    setIsLoadingDetails(true);

    let active = true;
    (async () => {
      try {
        const details = await api.getAvatarDetails(avatar.avatar_id);
        if (!active) return;
        setOnlineDetails(details);
      } catch (err) {
        console.error(`[AvatarModal] Failed to load details for ${avatar.avatar_id}:`, err);
      } finally {
        if (active) setIsLoadingDetails(false);
      }
    })();

    return () => {
      active = false;
    };
  }, [avatar?.avatar_id]);

  // Determine list of image URLs
  const imgUrls = onlineDetails?.images && onlineDetails.images.length > 0
    ? onlineDetails.images
    : [onlineDetails?.image_url, onlineDetails?.thumbnail_image_url].filter((u): u is string => !!u);

  const totalImages = imgUrls.length;

  // 2. Fetch image for current active index
  useEffect(() => {
    if (totalImages === 0) return;
    const targetUrl = imgUrls[currentImageIdx];
    if (!targetUrl) return;
    if (resolvedImages[currentImageIdx] || loadingImages[currentImageIdx]) return;

    let active = true;
    setLoadingImages((prev) => ({ ...prev, [currentImageIdx]: true }));

    api.fetchAvatarImage(targetUrl)
      .then((base64) => {
        if (active) {
          setResolvedImages((prev) => ({ ...prev, [currentImageIdx]: base64 }));
        }
      })
      .catch((err) => {
        console.error(`[AvatarModal] Failed to fetch image ${currentImageIdx}:`, err);
      })
      .finally(() => {
        if (active) {
          setLoadingImages((prev) => ({ ...prev, [currentImageIdx]: false }));
        }
      });

    return () => {
      active = false;
    };
  }, [currentImageIdx, totalImages, onlineDetails?.id]);

  if (!avatar) return null;

  const handleCopyId = () => {
    navigator.clipboard.writeText(avatar.avatar_id);
    setCopied(true);
    showToast('Avatar ID copied to clipboard!', 'info');
    setTimeout(() => setCopied(false), 2000);
  };

  const handleWear = async () => {
    if (!isLoggedIn) {
      showToast('Please log in to your VRChat account to wear avatars.', 'error');
      return;
    }
    try {
      setIsEquipping(true);
      await onWear(avatar.avatar_id);
      showToast(`Equipped ${avatar.name}!`, 'success');
    } catch (err: any) {
      showToast(`Failed to equip: ${err?.message || err}`, 'error');
    } finally {
      setIsEquipping(false);
    }
  };

  const handleOpenVrc = () => {
    api.openInBrowser(`https://vrchat.com/home/avatar/${avatar.avatar_id}`);
  };

  const nextImage = () => {
    if (totalImages > 0) {
      setCurrentImageIdx((prev) => (prev + 1) % totalImages);
    }
  };

  const prevImage = () => {
    if (totalImages > 0) {
      setCurrentImageIdx((prev) => (prev - 1 + totalImages) % totalImages);
    }
  };

  const currentImageUrl = resolvedImages[currentImageIdx] || null;
  const isCurrentImageLoading = loadingImages[currentImageIdx] || (isLoadingDetails && !currentImageUrl);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200">
      <div
        className="relative w-full max-w-2xl bg-[#0f1422] border border-gray-800 rounded-3xl overflow-hidden shadow-2xl flex flex-col max-h-[90vh]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Top Header Bar */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-800/80 bg-gray-900/60">
          <div className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-cyan-400" />
            <h2 className="text-base font-bold text-gray-100 truncate max-w-md">
              {avatar.name || 'Avatar Details'}
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Modal Scrollable Body */}
        <div className="p-6 overflow-y-auto space-y-6">
          {/* Main Visual Image Banner */}
          <div className="relative aspect-[4/3] w-full rounded-2xl overflow-hidden bg-gray-950 border border-gray-800 flex items-center justify-center">
            {currentImageUrl ? (
              <img
                src={currentImageUrl}
                alt={avatar.name}
                className="w-full h-full object-contain"
              />
            ) : isCurrentImageLoading ? (
              <div className="flex flex-col items-center gap-2 text-cyan-400">
                <Loader2 className="w-8 h-8 animate-spin" />
                <span className="text-xs text-gray-400">
                  {totalImages > 1 ? `Loading image ${currentImageIdx + 1} of ${totalImages}...` : 'Fetching preview...'}
                </span>
              </div>
            ) : (
              <div className="flex flex-col items-center gap-2 text-gray-600">
                <Sparkles className="w-12 h-12" />
                <span className="text-xs font-medium">No live image available</span>
              </div>
            )}

            {/* Image Carousel Controls */}
            {totalImages > 1 && (
              <>
                <button
                  onClick={(e) => { e.stopPropagation(); prevImage(); }}
                  className="absolute left-3 top-1/2 -translate-y-1/2 p-2 rounded-full bg-gray-900/80 hover:bg-gray-800 text-white border border-gray-700/60 transition-all shadow-lg"
                >
                  <ChevronLeft className="w-4 h-4" />
                </button>
                <button
                  onClick={(e) => { e.stopPropagation(); nextImage(); }}
                  className="absolute right-3 top-1/2 -translate-y-1/2 p-2 rounded-full bg-gray-900/80 hover:bg-gray-800 text-white border border-gray-700/60 transition-all shadow-lg"
                >
                  <ChevronRight className="w-4 h-4" />
                </button>
                <div className="absolute bottom-3 left-1/2 -translate-x-1/2 px-3 py-1 rounded-full bg-gray-900/80 border border-gray-700/60 text-xs text-gray-200 font-semibold">
                  {currentImageIdx + 1} / {totalImages}
                </div>
              </>
            )}

            {/* Platform Badges Overlay */}
            <div className="absolute top-3 left-3 flex gap-1.5">
              {avatar.platform & 1 ? (
                <span className="px-2.5 py-1 rounded-lg text-xs font-bold uppercase bg-cyan-950/90 border border-cyan-500/40 text-cyan-300 shadow-md">
                  PC
                </span>
              ) : null}
              {avatar.quest || avatar.platform & 2 ? (
                <span className="px-2.5 py-1 rounded-lg text-xs font-bold uppercase bg-emerald-950/90 border border-emerald-500/40 text-emerald-300 shadow-md">
                  Quest
                </span>
              ) : null}
              {avatar.ios || avatar.platform & 4 ? (
                <span className="px-2.5 py-1 rounded-lg text-xs font-bold uppercase bg-purple-950/90 border border-purple-500/40 text-purple-300 shadow-md">
                  iOS
                </span>
              ) : null}
              {onlineDetails?.release_status && (
                <span className="px-2.5 py-1 rounded-lg text-xs font-bold uppercase bg-gray-900/90 border border-gray-600/40 text-gray-300 shadow-md">
                  {onlineDetails.release_status}
                </span>
              )}
            </div>
          </div>

          {/* Quick Details & ID */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="p-3.5 rounded-xl bg-gray-900/60 border border-gray-800">
              <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1">
                Author
              </span>
              <p className="text-sm font-semibold text-gray-100">{avatar.author || 'Unknown'}</p>
            </div>

            <div className="p-3.5 rounded-xl bg-gray-900/60 border border-gray-800 flex items-center justify-between">
              <div>
                <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block mb-1">
                  Avatar ID
                </span>
                <p className="text-xs font-mono text-cyan-300 truncate max-w-[180px]">
                  {avatar.avatar_id}
                </p>
              </div>
              <button
                onClick={handleCopyId}
                className="p-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-gray-300 hover:text-white transition-colors"
                title="Copy Avatar ID"
              >
                {copied ? <Check className="w-4 h-4 text-emerald-400" /> : <Copy className="w-4 h-4" />}
              </button>
            </div>
          </div>

          {/* Description */}
          {avatar.description && (
            <div className="p-4 rounded-xl bg-gray-900/40 border border-gray-800 space-y-1">
              <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider block">
                Description
              </span>
              <p className="text-xs text-gray-300 leading-relaxed whitespace-pre-wrap">
                {avatar.description}
              </p>
            </div>
          )}

          {/* Performance & Compatibility Grid */}
          <div className="p-4 rounded-xl bg-gray-900/40 border border-gray-800 space-y-3">
            <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider flex items-center gap-1.5">
              <Shield className="w-3.5 h-3.5 text-cyan-400" />
              Performance Ratings
            </span>
            <div className="grid grid-cols-3 gap-3 text-center">
              <div className="p-2.5 rounded-xl bg-gray-900/90 border border-gray-800 flex flex-col items-center gap-1">
                <span className="text-[10px] text-gray-400 font-semibold uppercase tracking-wider">PC</span>
                <VrcRankIcon rank={avatar.pc_rating} className="w-5 h-5 my-0.5" />
                <span className="text-xs font-bold text-cyan-300">
                  {RATING_NAMES[avatar.pc_rating] || 'Unknown'}
                </span>
              </div>
              <div className="p-2.5 rounded-xl bg-gray-900/90 border border-gray-800 flex flex-col items-center gap-1">
                <span className="text-[10px] text-gray-400 font-semibold uppercase tracking-wider">Quest</span>
                <VrcRankIcon rank={avatar.quest_rating} className="w-5 h-5 my-0.5" />
                <span className="text-xs font-bold text-emerald-300">
                  {RATING_NAMES[avatar.quest_rating] || 'Unknown'}
                </span>
              </div>
              <div className="p-2.5 rounded-xl bg-gray-900/90 border border-gray-800 flex flex-col items-center gap-1">
                <span className="text-[10px] text-gray-400 font-semibold uppercase tracking-wider">iOS</span>
                <VrcRankIcon rank={avatar.ios_rating} className="w-5 h-5 my-0.5" />
                <span className="text-xs font-bold text-purple-300">
                  {RATING_NAMES[avatar.ios_rating] || 'Unknown'}
                </span>
              </div>
            </div>
          </div>

          {/* Tags */}
          {avatar.tags && avatar.tags.length > 0 && (
            <div className="space-y-2">
              <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider flex items-center gap-1.5">
                <Tag className="w-3.5 h-3.5 text-cyan-400" />
                Tags & Categories
              </span>
              <div className="flex flex-wrap gap-1.5">
                {avatar.tags.map((tag, i) => (
                  <span
                    key={i}
                    className="px-2.5 py-1 rounded-lg bg-gray-900 border border-gray-700/80 text-xs font-medium text-cyan-200"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Modal Action Footer */}
        <div className="flex items-center justify-between px-6 py-4 border-t border-gray-800/80 bg-gray-900/80">
          <div className="flex items-center gap-2">
            <button
              onClick={handleOpenVrc}
              className="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-gray-800/80 hover:bg-gray-800 text-gray-300 hover:text-white border border-gray-700/60 text-xs font-semibold transition-all"
            >
              <ExternalLink className="w-4 h-4" />
              <span>Open on VRChat</span>
            </button>

            {onToggleFavorite && avatar && (
              <button
                onClick={() => onToggleFavorite(avatar)}
                className={`flex items-center gap-1.5 px-4 py-2.5 rounded-xl border text-xs font-bold transition-all shadow-sm active:scale-95 ${
                  isFavorite
                    ? 'bg-amber-950/80 border-amber-500/80 text-amber-300 shadow-[0_0_12px_rgba(251,191,36,0.35)]'
                    : 'bg-gray-800/80 hover:bg-gray-700 text-gray-300 hover:text-amber-300 border-gray-700/60'
                }`}
              >
                <Star
                  className={`w-4 h-4 transition-transform ${
                    isFavorite ? 'fill-amber-400 text-amber-400 scale-110' : ''
                  }`}
                />
                <span>{isFavorite ? 'Favorited' : 'Favorite'}</span>
              </button>
            )}
          </div>

          <button
            onClick={handleWear}
            disabled={isEquipping}
            className="flex items-center gap-2 px-6 py-2.5 rounded-xl bg-gradient-to-r from-cyan-600 to-teal-600 hover:from-cyan-500 hover:to-teal-500 text-white text-xs font-bold shadow-lg shadow-cyan-600/30 transition-all disabled:opacity-50"
          >
            {isEquipping ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Shirt className="w-4 h-4" />
            )}
            <span>Wear This Avatar</span>
          </button>
        </div>
      </div>
    </div>
  );
};
