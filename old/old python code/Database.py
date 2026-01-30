import tkinter as tk
from tkinter import ttk
import requests
import json
from tqdm import tqdm
import logging
from concurrent.futures import ThreadPoolExecutor
import threading
import time
import os

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s"
)

CIPHER = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ=+"

# URLs for each platform
URLS = {
    "PC": "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb.txt",
    "Quest": "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_qst.txt",
    "iOS": "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_ios.txt"
}

class DatabaseLoader:
    def __init__(self):
        logging.info("Initializing DatabaseLoader")
        self.root = tk.Tk()
        self.root.title("VRChat Avatar Database Loader")
        self.root.geometry("600x400")
        
        # Create main frame
        self.main_frame = ttk.Frame(self.root, padding="10")
        self.main_frame.pack(fill="both", expand=True)
        
        # Title
        ttk.Label(self.main_frame, text="Loading VRChat Avatars Database", 
                 font=("Arial", 14, "bold")).pack(pady=10)
        
        # Progress section
        self.progress_frame = ttk.Frame(self.main_frame)
        self.progress_frame.pack(fill="x", pady=10)
        
        # Progress label
        self.progress_label = ttk.Label(self.progress_frame, text="")
        self.progress_label.pack(pady=5)
        
        # Progress bar
        self.progress_var = tk.DoubleVar()
        self.progress_bar = ttk.Progressbar(self.progress_frame, 
                                          variable=self.progress_var,
                                          maximum=100,
                                          length=500)
        self.progress_bar.pack(pady=5)
        
        # Status section
        self.status_frame = ttk.Frame(self.main_frame)
        self.status_frame.pack(fill="x", pady=10)
        
        # Status label
        self.status_label = ttk.Label(self.status_frame, text="", wraplength=500)
        self.status_label.pack(pady=5)
        
    def safe_update_progress(self, value, message):
        """Thread-safe progress update."""
        def update():
            self.progress_var.set(value)
            self.progress_label.config(text=message)
            self.root.update()
        self.root.after(0, update)
        
    def safe_update_status(self, text):
        """Thread-safe status update."""
        def update():
            self.status_label.config(text=text)
            self.root.update()
        self.root.after(0, update)
        
    def decode_avatar_id(self, crypt):
        """Decode avatar ID using cipher."""
        decrypt = [''] * 33
        new_format = (self.CIPHER.index(crypt[21]) >> 2) & 2

        for i in range(11):
            idx = i * 3
            first = self.CIPHER.index(crypt[i * 2])
            third = self.CIPHER.index(crypt[i * 2 + 1])

            decrypt[idx] = self.CIPHER[(first >> new_format) & 15]

            if new_format == 0:
                second = (first >> 2) & 12
            else:
                second = (first & 3) << 2

            decrypt[idx + 1] = self.CIPHER[second | ((third >> 4) & 3)]
            decrypt[idx + 2] = self.CIPHER[third & 15]

        decrypt.pop()

        for pos in (20, 16, 12, 8):
            decrypt.insert(pos, '-')

        return "avtr_" + ''.join(decrypt)

    def process_database_from_url(self, url, platform_name):
        """Process database URL with progress updates and error handling."""
        MAX_RETRIES = 3
        TIMEOUT = 10  # seconds
        
        for attempt in range(MAX_RETRIES):
            try:
                logging.info(f"Starting to process {platform_name} database from {url} (attempt {attempt + 1}/{MAX_RETRIES})")
                start_time = time.time()
                
                # Update progress with attempt number
                self.safe_update_progress(0, f"Downloading {platform_name} database... (attempt {attempt + 1})")
                
                # Add timeout to request
                response = requests.get(url, timeout=TIMEOUT)
                
                if response.status_code != 200:
                    if attempt < MAX_RETRIES - 1:
                        logging.warning(f"Attempt {attempt + 1} failed for {platform_name}: {response.status_code}. Retrying...")
                        continue
                    else:
                        logging.error(f"Failed to download {platform_name} after {MAX_RETRIES} attempts: {response.status_code}")
                        return []
                
                # Process successful response
                lines = response.text.splitlines()
                lines = lines[1:]  # skip metadata line
                total_lines = len(lines)
                
                logging.info(f"Found {total_lines} entries in {platform_name} database")
                decoded_entries = []
                
                self.safe_update_progress(0, f"Decoding {total_lines} {platform_name} entries...")
                
                for i, line in enumerate(lines):
                    if i % 10 == 0:  # Update progress every 10 entries
                        progress = (i / total_lines) * 100
                        self.safe_update_progress(progress, 
                                          f"Decoding {platform_name} entries: {i}/{total_lines}")
                    
                    parts = line.strip().split('\t')
                    if len(parts) < 4:
                        continue
                    parts = [p[::-1] for p in parts]  # Reverse each field
                    encoded_id, name, author, description = parts[:4]
                    decoded_id = self.decode_avatar_id(encoded_id)

                    entry = {
                        'avatar_id': decoded_id,
                        'name': name,
                        'author': author,
                        'description': description,
                        'platforms': [platform_name]
                    }
                    decoded_entries.append(entry)

                logging.info(f"Successfully processed {len(decoded_entries)} {platform_name} entries in {time.time() - start_time:.2f} seconds")
                return decoded_entries

            except requests.exceptions.Timeout:
                if attempt < MAX_RETRIES - 1:
                    logging.warning(f"Connection timed out for {platform_name} (attempt {attempt + 1}). Retrying...")
                    continue
                else:
                    logging.error(f"Connection timed out for {platform_name} after {MAX_RETRIES} attempts")
                    return []
            except requests.exceptions.RequestException as e:
                if attempt < MAX_RETRIES - 1:
                    logging.warning(f"Request error for {platform_name}: {str(e)}. Retrying...")
                    continue
                else:
                    logging.error(f"Request error for {platform_name}: {str(e)} after {MAX_RETRIES} attempts")
                    return []
            except Exception as e:
                logging.error(f"Unexpected error processing {platform_name} database: {e}", exc_info=True)
                return []

    def load_database(self):
        """Main database loading function."""
        logging.info("Starting database loading process")
        start_time = time.time()
        
        try:
            # Initialize progress
            self.safe_update_progress(0, "Initializing database loading...")
            
            all_avatars = {}
            total_platforms = len(URLS)
            platform_count = 0
            
            # Create thread pool for parallel downloads
            logging.info(f"Starting thread pool with {total_platforms} platforms")
            with ThreadPoolExecutor(max_workers=3) as executor:
                futures = []
                
                # Start loading each platform
                for platform_name, url in URLS.items():
                    logging.info(f"Submitting {platform_name} database download")
                    future = executor.submit(self.process_database_from_url, url, platform_name)
                    futures.append(future)
                    
                # Process results as they complete
                for future in futures:
                    platform_count += 1
                    avatars = future.result()
                    
                    # Update progress
                    progress = (platform_count / total_platforms) * 100
                    self.safe_update_progress(progress, 
                                      f"Processing platform {platform_count}/{total_platforms}")
                    
                    for avatar in avatars:
                        avatar_id = avatar['avatar_id']
                        if avatar_id not in all_avatars:
                            all_avatars[avatar_id] = avatar
                        else:
                            # Add platform if not already listed
                            if platform_name not in all_avatars[avatar_id]['platforms']:
                                all_avatars[avatar_id]['platforms'].append(platform_name)

            # Save results
            final_list = list(all_avatars.values())
            total_avatars = len(final_list)
            logging.info(f"Total unique avatars found: {total_avatars}")
            
            # Update UI with final status
            self.safe_update_progress(100, "Saving database...")
            self.safe_update_status(f"Total unique avatars: {total_avatars}")
            
            # Save to JSON
            db_path = os.path.join(os.path.dirname(__file__), "Avatar Data.json")
            logging.info(f"Saving database to {db_path}")
            try:
                with open(db_path, "w", encoding="utf-8") as f:
                    json.dump(final_list, f, ensure_ascii=False, indent=4)
                logging.info("Database saved successfully")
            except Exception as e:
                logging.error(f"Error saving database: {e}", exc_info=True)
                self.safe_update_status(f"Error saving database: {str(e)}")
                self.root.after(2000, self.root.destroy)
                return

            # Show success message
            self.safe_update_status(f"Database loaded successfully!\nTotal unique avatars: {total_avatars}")
            logging.info(f"Database loading completed in {time.time() - start_time:.2f} seconds")
            
            # Close window after a short delay
            self.root.after(2000, self.root.destroy)
            
        except Exception as e:
            logging.error(f"Error loading database: {e}", exc_info=True)
            self.safe_update_progress(100, "Error loading database")
            self.safe_update_status(f"Error: {str(e)}")
            self.root.after(2000, self.root.destroy)

    def run(self):
        """Run the loader window."""
        self.root.after(100, self.load_database)
        self.root.mainloop()

if __name__ == "__main__":
    loader = DatabaseLoader()
    loader.run()
