import requests
import json
from tqdm import tqdm

CIPHER = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+="

# URLs for each platform
URLS = {
    "PC": "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb.txt",
    "Quest": "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_qst.txt",
    "iOS": "https://gist.githubusercontent.com/Mwr247/a80c1f9060fc4fd46a8f00d589c47c5a/raw/pasavtrdb_ios.txt"
}

def decode_avatar_id(crypt):
    decrypt = [''] * 33
    new_format = (CIPHER.index(crypt[21]) >> 2) & 2

    for i in range(11):
        idx = i * 3
        first = CIPHER.index(crypt[i * 2])
        third = CIPHER.index(crypt[i * 2 + 1])

        decrypt[idx] = CIPHER[(first >> new_format) & 15]

        if new_format == 0:
            second = (first >> 2) & 12
        else:
            second = (first & 3) << 2

        decrypt[idx + 1] = CIPHER[second | ((third >> 4) & 3)]
        decrypt[idx + 2] = CIPHER[third & 15]

    decrypt.pop()

    for pos in (20, 16, 12, 8):
        decrypt.insert(pos, '-')

    return "avtr_" + ''.join(decrypt)

def process_database_from_url(url, platform_name):
    print(f"\nDownloading ({platform_name}): {url}")
    response = requests.get(url)
    if response.status_code != 200:
        print(f"Failed to download: {response.status_code}")
        return []

    lines = response.text.splitlines()
    lines = lines[1:]  # skip metadata line

    decoded_entries = []

    print(f"Decoding {len(lines)} {platform_name} entries...")
    for line in tqdm(lines, desc=f"Decoding {platform_name}", unit="avatar"):
        parts = line.strip().split('\t')
        if len(parts) < 4:
            continue
        parts = [p[::-1] for p in parts]  # Reverse each field
        encoded_id, name, author, description = parts[:4]
        decoded_id = decode_avatar_id(encoded_id)

        decoded_entries.append({
            'avatar_id': decoded_id,
            'name': name,
            'author': author,
            'description': description,
            'platforms': [platform_name]
        })

    return decoded_entries

def main():
    all_avatars = {}
    
    for platform_name, url in URLS.items():
        avatars = process_database_from_url(url, platform_name)
        for avatar in avatars:
            avatar_id = avatar['avatar_id']
            if avatar_id not in all_avatars:
                all_avatars[avatar_id] = avatar
            else:
                # If already exists, just add the platform if it's not already listed
                if platform_name not in all_avatars[avatar_id]['platforms']:
                    all_avatars[avatar_id]['platforms'].append(platform_name)

    final_list = list(all_avatars.values())

    print(f"\nTotal unique avatars decoded: {len(final_list)}")

    # Print first 5 decoded avatars
    print("\nShowing first 5 decoded avatars with platforms:")
    for avatar in final_list[:5]:
        print(f"{avatar['avatar_id']} | Platforms: {avatar['platforms']} | {avatar['name']} | {avatar['author']}")

    # Save to JSON
    with open("Avatar Data.json", "w", encoding="utf-8") as f:
        json.dump(final_list, f, ensure_ascii=False, indent=4)

    print("\nSaved to Avatar Data.json")

if __name__ == "__main__":
    main()
