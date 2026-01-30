import requests
import json
from datetime import datetime
from typing import Dict, List, Optional
import struct
import os
from pathlib import Path

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
        crypt = list(crypt)
        for i in range(len(crypt) - 1, -1, -1):
            k = crypt[i] ^ crypt[(i + len(crypt) - 1) % len(crypt)] ^ iv[i]
            crypt[i] = k

        decrypt = [f"{x:02x}" for x in crypt][::-1]
        decrypt.insert(8, '-')
        decrypt.insert(13, '-')
        decrypt.insert(18, '-')
        decrypt.insert(23, '-')
        return "avtr_" + ''.join(decrypt)

    def get_prismic_obj(self, url: str) -> Dict:
        print("Downloading avatar database...")
        response = requests.get(url)
        content = Reader(response.content)

        print("Parsing avatar database...")
        avatar_data = {
            'entries': [],
            'idMap': {},
            'avatarCount': 0,
            'authorCount': 0,
            'lastUpdate': datetime.now().strftime('%Y-%m-%d')
        }

        if not content.data:
            raise ValueError("Data has length zero")
        
        header = content.read_bytes(3).decode()
        if header != "PAS":
            raise ValueError("PAS Header not found")

        content.read_bytes(2)  # platform and format version
        avatar_data['avatarCount'] = content.read_int24()
        avatar_data['authorCount'] = content.read_int24()

        date_arr = content.read_bytes(2)
        date_num = ((date_arr[0] << 8) + date_arr[1]) >> 3
        year = ((date_num >> 9) + 16)
        month = ((date_num >> 5) & 15)
        day = (date_num & 31)
        avatar_data['lastUpdate'] = f"20{year:02d}-{month:02d}-{day:02d}"

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
                'platforms': ['PC']  # Default to PC
            }
            avatar_data['idMap'][avatar_id] = obj
            avatar_data['entries'].append(obj)

        return avatar_data

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

    def fetch_avatar_data(self) -> Dict:
        print("Fetching avatar data...")
        try:
            main_data = self.get_prismic_obj(self.urls[0])
            quest_ids = self.get_aux_prismic_obj(self.urls[1])
            ios_ids = self.get_aux_prismic_obj(self.urls[2])

            print("Marking platform-specific avatars...")
            self.mark_avatars(main_data, quest_ids, 'Quest')
            self.mark_avatars(main_data, ios_ids, 'iOS')

            # Save to cache
            cache_path = self.cache_dir / 'avatar_data.json'
            with open(cache_path, 'w', encoding='utf-8') as f:
                json.dump(main_data, f, ensure_ascii=False, indent=2)

            return main_data

        except Exception as e:
            print(f"Error fetching avatar data: {e}")
            return {}

    def mark_avatars(self, main_data: Dict, ids: List[str], platform: str):
        nfa = []
        for avatar_id in ids:
            entry = main_data['idMap'].get(avatar_id)
            if not entry:
                nfa.append(avatar_id)
                continue
            if platform not in entry['platforms']:
                entry['platforms'].append(platform)

        print(f"Marked {len(ids) - len(nfa)} {platform} avatars.")
        if nfa:
            print(f"Found {len(nfa)} missing from the main list")

    def search_avatars(self, query: str, search_type: str = 'all') -> List[Dict]:
        """Search avatars by name, author, or description"""
        results = []
        query = query.lower()
        
        for entry in self.data['entries'][::-1]:  # Newest first
            if search_type == 'name' and query in entry['name'].lower():
                results.append(entry)
            elif search_type == 'author' and query in entry['author'].lower():
                results.append(entry)
            elif search_type == 'description' and query in entry['description'].lower():
                results.append(entry)
            elif search_type == 'all':
                if (query in entry['name'].lower() or
                    query in entry['author'].lower() or
                    query in entry['description'].lower()):
                    results.append(entry)

        return results

    def load_cached_data(self) -> Optional[Dict]:
        cache_path = self.cache_dir / 'avatar_data.json'
        if cache_path.exists():
            try:
                with open(cache_path, 'r', encoding='utf-8') as f:
                    return json.load(f)
            except Exception:
                return None
        return None

if __name__ == "__main__":
    db = AvatarDatabase()
    data = db.fetch_avatar_data()
    
    if data:
        print(f"Loaded {len(data['entries'])} avatars")
        print(f"Last update: {data['lastUpdate']}")
        print(f"Authors: {data['authorCount']}")
        print(f"Avatars: {data['avatarCount']}")
