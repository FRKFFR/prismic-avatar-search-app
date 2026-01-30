import tkinter as tk
from tkinter import ttk
import threading
import requests
import json
from datetime import datetime
from typing import Dict, List, Optional
import struct
import os
from pathlib import Path
import time
import shutil
import subprocess
import sys


def run_rust_downloader(extra_args: Optional[List[str]] = None) -> int:
    """Launch the Rust GUI downloader, building it if necessary."""
    project_dir = Path(__file__).resolve().parent / "rust_gui"
    if not project_dir.exists():
        raise SystemExit(f"Rust project directory not found: {project_dir}")

    binary_name = "vrchat_avatar_gui.exe" if os.name == "nt" else "vrchat_avatar_gui"
    binary_path = project_dir / "target" / "release" / binary_name

    cmd: List[str]
    if binary_path.exists():
        cmd = [str(binary_path)]
    else:
        cargo = shutil.which("cargo")
        if cargo is None:
            raise SystemExit("Rust toolchain (cargo) is required but was not found in PATH.")
        cmd = [cargo, "run", "--release", "--"]

    if extra_args:
        cmd.extend(extra_args)

    print(f"Launching Rust downloader via: {' '.join(cmd)}")
    try:
        completed = subprocess.run(cmd, cwd=project_dir, check=True)
        return completed.returncode
    except subprocess.CalledProcessError as exc:
        raise SystemExit(f"Rust downloader exited with code {exc.returncode}") from exc

class DatabaseUI:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("VRChat Avatar Database Downloader")
        self.root.geometry("600x400")
        
        # Create main frame
        self.main_frame = ttk.Frame(self.root, padding="10")
        self.main_frame.grid(row=0, column=0, sticky=(tk.W, tk.E, tk.N, tk.S))
        
        # Progress bars and labels
        self.progress_labels = []
        self.progress_bars = []
        
        # Create UI elements
        self.create_ui_elements()
        
        # Database instance
        self.db = AvatarDatabase()
        
        # Status label
        self.status_label = ttk.Label(self.main_frame, text="Starting download...")
        self.status_label.grid(row=0, column=0, columnspan=2, pady=5)
        
        # Progress bars for each step
        self.create_progress_bar("Downloading Main Database", 1)
        self.create_progress_bar("Downloading Quest Database", 2)
        self.create_progress_bar("Downloading iOS Database", 3)
        self.create_progress_bar("Processing Data", 4)
        
        # Results display
        self.results_frame = ttk.LabelFrame(self.main_frame, text="Results", padding="5")
        self.results_frame.grid(row=5, column=0, columnspan=2, pady=10, sticky=(tk.W, tk.E))
        
        self.results_labels = {}
        self.create_result_label("Avatars:", "avatars")
        self.create_result_label("Authors:", "authors")
        self.create_result_label("Last Update:", "last_update")
        
        # Start download automatically
        threading.Thread(target=self.download_data, daemon=True).start()
        
        self.root.mainloop()

    def create_ui_elements(self):
        # Configure grid weights
        self.root.columnconfigure(0, weight=1)
        self.root.rowconfigure(0, weight=1)
        
        self.main_frame.columnconfigure(0, weight=1)
        self.main_frame.columnconfigure(1, weight=1)

    def create_progress_bar(self, text: str, row: int):
        label = ttk.Label(self.main_frame, text=text)
        label.grid(row=row, column=0, padx=5, pady=5, sticky=tk.W)
        self.progress_labels.append(label)
        
        progress = ttk.Progressbar(self.main_frame, length=400, mode='determinate')
        progress.grid(row=row, column=1, padx=5, pady=5, sticky=tk.W)
        self.progress_bars.append(progress)

    def create_result_label(self, text: str, key: str):
        label = ttk.Label(self.results_frame, text=text)
        label.grid(row=len(self.results_labels), column=0, padx=5, pady=2, sticky=tk.W)
        
        value_label = ttk.Label(self.results_frame, text="")
        value_label.grid(row=len(self.results_labels), column=1, padx=5, pady=2, sticky=tk.W)
        self.results_labels[key] = value_label

    def update_progress(self, step: int, value: int):
        if 0 <= step < len(self.progress_bars):
            self.progress_bars[step]['value'] = value
            self.root.update_idletasks()

    def update_status(self, text: str):
        self.status_label['text'] = text
        self.root.update_idletasks()

    def update_result(self, key: str, value: str):
        if key in self.results_labels:
            self.results_labels[key]['text'] = value
            self.root.update_idletasks()

    def start_download(self):
        self.start_button['state'] = 'disabled'
        self.update_status("Downloading and processing database...")
        
        # Reset progress bars
        for bar in self.progress_bars:
            bar['value'] = 0
            
        # Start download in a separate thread
        threading.Thread(target=self.download_data, daemon=True).start()

    def download_data(self):
        try:
            # Process all databases
            self.update_status("Processing databases...")
            main_data = self.db.process_database()
            
            # Save to cache
            cache_path = self.db.cache_dir / 'avatar_data.json'
            with open(cache_path, 'w', encoding='utf-8') as f:
                json.dump(main_data, f, ensure_ascii=False, indent=2)
            
            # Update progress bars
            for i in range(4):
                self.update_progress(i, 100)
            
            # Update results
            self.update_status("Download complete!")
            self.update_result("avatars", str(len(main_data)))
            self.update_result("authors", str(len(set(avatar['author'] for avatar in main_data))))
            self.update_result("last_update", datetime.now().strftime('%Y-%m-%d'))
            
            # Close the window after 2 seconds
            self.root.after(2000, self.root.destroy)
            
        except Exception as e:
            self.update_status(f"Error: {str(e)}")
            # Close the window after 5 seconds if there's an error
            self.root.after(5000, self.root.destroy)

class Reader:
    def __init__(self, data: bytes):
        self.data = data
        self.position = 0

    def read_byte(self) -> int:
        if self.position >= len(self.data):
            raise ValueError("Attempted to read beyond end of data")
        value = self.data[self.position]
        self.position += 1
        return value

    def read_bytes(self, amount: int) -> bytes:
        if self.position + amount > len(self.data):
            raise ValueError("Attempted to read beyond end of data")
        bytes_ = self.data[self.position:self.position + amount]
        self.position += amount
        return bytes_

    def read_int_array(self, n: int) -> List[int]:
        total_bytes = n * 4
        if self.position + total_bytes > len(self.data):
            raise ValueError("Attempted to read beyond end of data")
        
        result = []
        for _ in range(n):
            value = struct.unpack('<i', self.data[self.position:self.position+4])[0]
            result.append(value)
            self.position += 4
        return result

    def read_int24(self) -> int:
        if self.position + 3 > len(self.data):
            raise ValueError("Attempted to read beyond end of data")
        
        bytes_ = self.read_bytes(3)
        return (bytes_[0] << 16) | (bytes_[1] << 8) | bytes_[2]

    def remaining(self) -> int:
        return len(self.data) - self.position

class AvatarDatabase:
    def __init__(self):
        self.urls = [
            "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb.txt",
            "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_qst.txt",
            "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_ios.txt"
        ]
        self.static_bytes = [208, 29, 107, 36, 251, 69, 122, 14, 67, 204, 171, 246, 106, 38, 183, 224]
        self.cache_dir = Path("cache")
        self.cache_dir.mkdir(exist_ok=True)

    def decode_avatar_id(self, crypt: bytes, iv: bytes) -> str:
        # Process each byte with XOR operations including static_bytes
        decoded = []
        for i in range(16):
            decoded.append(crypt[i] ^ iv[i] ^ self.static_bytes[i])
        
        # Convert to hex and reverse each byte
        hex_bytes = [f"{x:02x}" for x in decoded]
        # Format as proper UUID (8-4-4-4-12)
        uuid = (
            f"{''.join(hex_bytes[0:4])}-"
            f"{''.join(hex_bytes[4:6])}-"
            f"{''.join(hex_bytes[6:8])}-"
            f"{''.join(hex_bytes[8:10])}-"
            f"{''.join(hex_bytes[10:16])}"
        )

        return f"avtr_{uuid}"

    def get_prismic_obj(self, url: str, platform: str) -> List[Dict]:
        response = requests.get(url)
        content = Reader(response.content)

        if not content.data:
            raise ValueError("Data has length zero")
        
        header = content.read_bytes(3).decode()
        if header != "PAS":
            raise ValueError("PAS Header not found")

        content.read_bytes(2)  # Skip platform and format version
        avatar_count = content.read_int24()
        author_count = content.read_int24()

        date_arr = content.read_bytes(2)
        date_num = ((date_arr[0] << 8) + date_arr[1]) >> 3
        year = ((date_num >> 9) + 16)
        month = ((date_num >> 5) & 15)
        day = (date_num & 31)
        last_update = f"20{year:02d}-{month:02d}-{day:02d}"

        file_avatars = content.read_int24()
        file_authors = content.read_int24()

        flag_size = content.read_byte()
        random_bytes = content.read_bytes(16)
        dynamic_bytes = [e ^ self.static_bytes[i] for i, e in enumerate(random_bytes)]

        data_size = file_avatars * 16
        avatar_ids = content.read_bytes(data_size)
        flag_data_size = file_avatars * flag_size
        flags = content.read_int_array(file_avatars)
        author_ids = content.read_int_array(file_avatars)

        strings = content.read_bytes(content.remaining()).decode('utf-8').split('\n')
        if len(strings) < 2:
            raise ValueError("Malformed string block")

        author_names = strings[0].split('\r')
        avatar_names = strings[1].split('\r')

        decoded_entries = []
        for i in range(file_avatars):
            avatar_id = self.decode_avatar_id(
                avatar_ids[i * 16:(i * 16) + 16],
                dynamic_bytes
            )
            name_desc = avatar_names[i].split('\t')
            obj = {
                'avatar_id': avatar_id,
                'name': name_desc[0][::-1],
                'author': author_names[author_ids[i] & 524287][::-1],
                'description': name_desc[1][::-1] if len(name_desc) > 1 else '',
                'platforms': [platform]
            }
            decoded_entries.append(obj)

        print(f"Decoded {len(decoded_entries)} {platform} entries")
        return decoded_entries

    def process_database(self):
        all_avatars = {}
        
        for platform, url in zip(['PC', 'Quest', 'iOS'], self.urls):
            print(f"\nProcessing {platform} database...")
            try:
                avatars = self.get_prismic_obj(url, platform)
                for avatar in avatars:
                    avatar_id = avatar['avatar_id']
                    if avatar_id not in all_avatars:
                        all_avatars[avatar_id] = avatar
                    else:
                        # If already exists, just add the platform if it's not already listed
                        if platform not in all_avatars[avatar_id]['platforms']:
                            all_avatars[avatar_id]['platforms'].append(platform)
            except Exception as e:
                print(f"Error processing {platform} database: {e}")

        final_list = list(all_avatars.values())
        return final_list

    def get_aux_prismic_obj(self, url: str) -> List[str]:
        response = requests.get(url)
        content = Reader(response.content)

        if not content.data:
            raise ValueError("Data has length zero")
        
        header = content.read_bytes(3).decode()
        if header != "PAS":
            raise ValueError("PAS Header not found")

        # Skip unnecessary bytes
        content.read_bytes(2 + 3 + 3 + 2)
        file_avatars = content.read_int24()
        content.read_bytes(3 + 1)

        ids = []
        dynamic_bytes = content.read_bytes(16)
        dynamic_bytes = [e ^ self.static_bytes[i] for i, e in enumerate(dynamic_bytes)]
        avatar_ids = content.read_bytes(file_avatars * 16)

        for i in range(file_avatars):
            avatar_id = self.decode_avatar_id(
                avatar_ids[i*16:(i*16)+16],
                dynamic_bytes
            )
            ids.append(avatar_id)

        return ids

    def mark_avatars(self, main_data: Dict, ids: List[str], platform: str):
        nfa = []
        duplicates = 0
        for avatar_id in ids:
            entry = main_data['idMap'].get(avatar_id)
            if not entry:
                nfa.append(avatar_id)
                continue
            
            # Only add platform if it's not already there
            if platform not in entry['platforms']:
                entry['platforms'].append(platform)
            else:
                duplicates += 1

        print(f"Marked {len(ids) - len(nfa)} {platform} avatars.")
        if nfa:
            print(f"Found {len(nfa)} missing from the main list")
        if duplicates > 0:
            print(f"Skipped {duplicates} duplicate {platform} entries")

if __name__ == "__main__":
    app = DatabaseUI()
