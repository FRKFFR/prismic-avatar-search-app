import React from 'react';
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, SearchX } from 'lucide-react';
import type { AvatarEntry } from '../types';
import { AvatarCard } from './AvatarCard';

interface AvatarGridProps {
  avatars: AvatarEntry[];
  isLoading: boolean;
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
  onPageChange: (newPage: number) => void;
  onSelect: (avatar: AvatarEntry) => void;
  onWear: (avatarId: string) => Promise<void>;
  isLoggedIn: boolean;
  showOldImages: boolean;
  favoriteIds?: Set<string>;
  onToggleFavorite?: (avatar: AvatarEntry) => void;
}

export const AvatarGrid: React.FC<AvatarGridProps> = ({
  avatars,
  isLoading,
  total,
  page,
  pageSize,
  totalPages,
  onPageChange,
  onSelect,
  onWear,
  isLoggedIn,
  showOldImages,
  favoriteIds,
  onToggleFavorite,
}) => {
  return (
    <div className="flex-1 flex flex-col p-4 lg:p-6 overflow-y-auto">
      {/* Top Search Results Header */}
      <div className="flex items-center justify-between mb-4 pb-2 border-b border-gray-800/80">
        <div className="text-xs text-gray-400">
          Showing{' '}
          <span className="font-semibold text-gray-200">
            {total === 0 ? 0 : page * pageSize + 1} - {Math.min((page + 1) * pageSize, total)}
          </span>{' '}
          of <span className="font-semibold text-cyan-400">{total.toLocaleString()}</span> avatars
        </div>

        {totalPages > 1 && (
          <div className="text-xs text-gray-400">
            Page <span className="font-semibold text-gray-200">{page + 1}</span> of{' '}
            <span className="font-semibold text-gray-200">{totalPages}</span>
          </div>
        )}
      </div>

      {/* Grid or Skeleton or Empty State */}
      {isLoading ? (
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5 gap-4">
          {Array.from({ length: 12 }).map((_, i) => (
            <div
              key={i}
              className="bg-gray-900/60 border border-gray-800/80 rounded-2xl overflow-hidden animate-pulse flex flex-col"
            >
              <div className="aspect-video bg-gray-800/60" />
              <div className="p-4 space-y-2.5">
                <div className="h-4 bg-gray-800 rounded w-3/4" />
                <div className="h-3 bg-gray-800/80 rounded w-1/2" />
                <div className="h-8 bg-gray-800/40 rounded mt-4" />
              </div>
            </div>
          ))}
        </div>
      ) : avatars.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center p-12 text-center">
          <div className="w-16 h-16 rounded-2xl bg-gray-900 border border-gray-800 flex items-center justify-center text-gray-500 mb-4 shadow-xl">
            <SearchX className="w-8 h-8" />
          </div>
          <h3 className="text-base font-semibold text-gray-200">No avatars found</h3>
          <p className="text-xs text-gray-500 max-w-sm mt-1 leading-relaxed">
            Try adjusting your search terms or clearing some platform or performance filters.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5 gap-4">
          {avatars.map((avatar) => (
            <AvatarCard
              key={avatar.avatar_id}
              avatar={avatar}
              onSelect={onSelect}
              onWear={onWear}
              isLoggedIn={isLoggedIn}
              showOldImages={showOldImages}
              isFavorite={favoriteIds?.has(avatar.avatar_id)}
              onToggleFavorite={onToggleFavorite}
            />
          ))}
        </div>
      )}

      {/* Pagination Bar */}
      {totalPages > 1 && (
        <div className="mt-8 pt-4 border-t border-gray-800/80 flex items-center justify-center gap-2 select-none">
          <button
            onClick={() => onPageChange(0)}
            disabled={page === 0}
            className="p-2 rounded-xl bg-gray-900 border border-gray-800 hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed text-gray-300 transition-colors"
            title="First page"
          >
            <ChevronsLeft className="w-4 h-4" />
          </button>
          <button
            onClick={() => onPageChange(page - 1)}
            disabled={page === 0}
            className="p-2 rounded-xl bg-gray-900 border border-gray-800 hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed text-gray-300 transition-colors"
            title="Previous page"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>

          <span className="px-4 py-1.5 text-xs font-semibold text-gray-300 bg-gray-900/80 border border-gray-800 rounded-xl">
            {page + 1} / {totalPages}
          </span>

          <button
            onClick={() => onPageChange(page + 1)}
            disabled={page >= totalPages - 1}
            className="p-2 rounded-xl bg-gray-900 border border-gray-800 hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed text-gray-300 transition-colors"
            title="Next page"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
          <button
            onClick={() => onPageChange(totalPages - 1)}
            disabled={page >= totalPages - 1}
            className="p-2 rounded-xl bg-gray-900 border border-gray-800 hover:bg-gray-800 disabled:opacity-40 disabled:cursor-not-allowed text-gray-300 transition-colors"
            title="Last page"
          >
            <ChevronsRight className="w-4 h-4" />
          </button>
        </div>
      )}
    </div>
  );
};
