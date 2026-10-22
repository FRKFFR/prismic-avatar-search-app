import React, { useState, useEffect } from 'react';
import { X, Settings, Image as ImageIcon, Trash2, Check, RefreshCw, Download } from 'lucide-react';
import { api } from '../api';
import { useToast } from './Toast';
import type { AppSettings, UpdateInfo } from '../types';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSettingsSaved?: () => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  onSettingsSaved,
}) => {
  const { showToast } = useToast();
  const [settings, setSettings] = useState<AppSettings>({ show_old_images: false });
  const [isClearingCache, setIsClearingCache] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateStatus, setUpdateStatus] = useState<'idle' | 'latest' | 'error'>('idle');

  useEffect(() => {
    if (isOpen) {
      api.getAppSettings().then((s) => setSettings(s)).catch(() => {});
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleToggleOldImages = async () => {
    const updated = { ...settings, show_old_images: !settings.show_old_images };
    setSettings(updated);
    setIsSaving(true);
    try {
      await api.updateAppSettings(updated);
      showToast(
        updated.show_old_images
          ? 'Historical image versions enabled'
          : 'Historical image versions disabled',
        'info'
      );
      if (onSettingsSaved) onSettingsSaved();
    } catch (err: any) {
      showToast(`Failed to update settings: ${err?.message || err}`, 'error');
    } finally {
      setIsSaving(false);
    }
  };

  const handleClearCache = async () => {
    setIsClearingCache(true);
    try {
      await api.clearImageCache();
      showToast('Image and details cache cleared successfully!', 'success');
    } catch (err: any) {
      showToast(`Failed to clear cache: ${err?.message || err}`, 'error');
    } finally {
      setIsClearingCache(false);
    }
  };

  const handleCheckUpdate = async () => {
    setIsCheckingUpdate(true);
    try {
      const info = await api.checkForUpdates();
      setUpdateInfo(info.update_available ? info : null);
      setUpdateStatus(info.update_available ? 'idle' : 'latest');
    } catch (_) {
      setUpdateInfo(null);
      setUpdateStatus('error');
    } finally {
      setIsCheckingUpdate(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-200">
      <div
        className="relative w-full max-w-lg bg-[#0f1422] border border-gray-800 rounded-3xl overflow-hidden shadow-2xl flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Modal Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-800/80 bg-gray-900/60">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-xl bg-cyan-950/60 border border-cyan-800/40 flex items-center justify-center text-cyan-400">
              <Settings className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-gray-100">Settings</h2>
              <p className="text-[11px] text-gray-400">Application preferences & cache</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-xl hover:bg-gray-800 text-gray-400 hover:text-white transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Modal Body */}
        <div className="p-6 space-y-5">
          {/* Historical Avatar Images Toggle */}
          <div className="p-4 rounded-2xl bg-gray-900/40 border border-gray-800 flex items-start justify-between gap-4">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <ImageIcon className="w-4 h-4 text-cyan-400" />
                <h3 className="text-xs font-bold text-gray-200">Show Historical Image Revisions</h3>
              </div>
              <p className="text-[11px] text-gray-400 leading-relaxed pr-2">
                When enabled, avatar details will also fetch superseded image versions from earlier avatar file updates in addition to the gallery pictures.
              </p>
            </div>
            <button
              onClick={handleToggleOldImages}
              disabled={isSaving}
              className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none ${
                settings.show_old_images ? 'bg-cyan-500' : 'bg-gray-700'
              }`}
            >
              <span
                className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                  settings.show_old_images ? 'translate-x-5' : 'translate-x-0'
                }`}
              />
            </button>
          </div>

          {/* Clear Cache Card */}
          <div className="p-4 rounded-2xl bg-gray-900/40 border border-gray-800 flex items-start justify-between gap-4">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Trash2 className="w-4 h-4 text-amber-400" />
                <h3 className="text-xs font-bold text-gray-200">Clear Image & Data Cache</h3>
              </div>
              <p className="text-[11px] text-gray-400 leading-relaxed pr-2">
                Wipe all downloaded thumbnails and cached avatar metadata. Fresh images will download on your next browse.
              </p>
            </div>
            <button
              onClick={handleClearCache}
              disabled={isClearingCache}
              className="px-3.5 py-1.5 rounded-xl bg-gray-800 hover:bg-rose-950/40 hover:text-rose-300 border border-gray-700 hover:border-rose-800/50 text-xs font-semibold text-gray-200 transition-all flex items-center gap-1.5 shrink-0 disabled:opacity-50"
            >
              {isClearingCache ? (
                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Trash2 className="w-3.5 h-3.5" />
              )}
              <span>Clear</span>
            </button>
          </div>
          {/* App Updates Card */}
          <div className="p-4 rounded-2xl bg-gray-900/40 border border-gray-800 flex items-start justify-between gap-4">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Download className="w-4 h-4 text-cyan-400" />
                <h3 className="text-xs font-bold text-gray-200">App Updates</h3>
              </div>
              <p className="text-[11px] text-gray-400 leading-relaxed pr-2">
                {updateInfo
                  ? `Version ${updateInfo.latest_version} is available (you have ${updateInfo.current_version}).`
                  : updateStatus === 'latest'
                  ? 'You are running the latest version.'
                  : updateStatus === 'error'
                  ? 'Could not check for updates right now.'
                  : 'Check GitHub for a newer release.'}
              </p>
            </div>
            {updateInfo ? (
              <button
                onClick={() => api.openInBrowser(updateInfo.release_url)}
                className="px-3.5 py-1.5 rounded-xl bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-semibold transition-all flex items-center gap-1.5 shrink-0"
              >
                <Download className="w-3.5 h-3.5" />
                <span>Download</span>
              </button>
            ) : (
              <button
                onClick={handleCheckUpdate}
                disabled={isCheckingUpdate}
                className="px-3.5 py-1.5 rounded-xl bg-gray-800 hover:bg-gray-700 border border-gray-700 text-xs font-semibold text-gray-200 transition-all flex items-center gap-1.5 shrink-0 disabled:opacity-50"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${isCheckingUpdate ? 'animate-spin' : ''}`} />
                <span>Check</span>
              </button>
            )}
          </div>
        </div>

        {/* Modal Footer */}
        <div className="flex items-center justify-end px-6 py-4 border-t border-gray-800/80 bg-gray-900/80">
          <button
            onClick={onClose}
            className="flex items-center gap-2 px-5 py-2 rounded-xl bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-bold transition-all shadow-md shadow-cyan-600/20"
          >
            <Check className="w-4 h-4" />
            <span>Done</span>
          </button>
        </div>
      </div>
    </div>
  );
};
