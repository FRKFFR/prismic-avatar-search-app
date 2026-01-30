import tkinter as tk
from tkinter import ttk, messagebox
import requests
import json
import logging
import os
from datetime import datetime  

# Setup logging
logging.basicConfig(
    level=logging.DEBUG,
    format="%(asctime)s - %(levelname)s - %(message)s"
)

# Config file path
CONFIG_FILE = "config.json"

# VRChat API endpoints
API_BASE = "https://api.vrchat.cloud/api/1"

# Create config file if it doesn't exist
def create_config():
    config = {
        "auth_cookie": "",
        "user_id": "",
        "last_login": None,
        "remember_me": False
    }
    with open(CONFIG_FILE, 'w') as f:
        json.dump(config, f, indent=4)

# Load config
def load_config():
    if not os.path.exists(CONFIG_FILE):
        create_config()
    with open(CONFIG_FILE, 'r') as f:
        return json.load(f)

# Save config
def save_config(config):
    with open(CONFIG_FILE, 'w') as f:
        json.dump(config, f, indent=4)

def verify_2fa_code(code, auth_cookie, two_factor_type):
    try:
        headers = {"User-Agent": "VRChatAPI/1.0"}
        cookies = {"auth": auth_cookie}
        
        # Verify 2FA code
        if two_factor_type == "emailOtp":
            endpoint = f"{API_BASE}/auth/twofactorauth/emailotp/verify"
        else:  # totp
            endpoint = f"{API_BASE}/auth/twofactorauth/totp/verify"
            
        response = requests.post(
            endpoint,
            headers=headers,
            cookies=cookies,
            json={"code": code}
        )
        
        if response.status_code == 200:
            data = response.json()
            if data.get("verified") is True:
                logging.info("2FA code verified successfully.")
                
                # Get user info to confirm full authentication
                verify = requests.get(
                    f"{API_BASE}/auth/user",
                    headers=headers,
                    cookies=cookies
                )
                
                if verify.status_code == 200:
                    user_id = verify.json().get("id")
                    return True, user_id
                else:
                    logging.error(f"Failed to verify user: {verify.status_code}")
                    return False, None
        else:
            logging.error(f"2FA verification failed: {response.status_code}")
            return False, None
            
    except Exception as e:
        logging.error(f"Error verifying 2FA code: {e}")
        return False, None

def login_window():
    def login():
        username = username_entry.get()
        password = password_entry.get()
        remember_me = remember_var.get()
        
        if not username or not password:
            messagebox.showerror("Error", "Please enter username and password")
            return
            
        try:
            headers = {"User-Agent": "VRChatAPI/1.0"}
            
            # Get auth cookie
            auth_response = requests.get(
                f"{API_BASE}/auth/user",
                headers=headers,
                auth=(username, password)
            )
            
            if auth_response.status_code == 200:
                auth_cookie = auth_response.cookies.get("auth")
                
                # Check if 2FA is required
                if auth_response.json().get("requiresTwoFactorAuth"):
                    two_factor_type = auth_response.json()["requiresTwoFactorAuth"][0]
                    logging.info(f"2FA required. Type: {two_factor_type}")
                    
                    # Show 2FA code input
                    code_window = tk.Toplevel(root)
                    code_window.title("2FA Verification")
                    code_window.geometry("300x150")
                    
                    code_label = ttk.Label(code_window, text="Enter 2FA code:")
                    code_label.pack(pady=10)
                    
                    code_entry = ttk.Entry(code_window, show="*")
                    code_entry.pack(pady=10)
                    
                    def verify():
                        code = code_entry.get()
                        if not code:
                            messagebox.showerror("Error", "Please enter the 2FA code")
                            return
                        
                        success, user_id = verify_2fa_code(code, auth_cookie, two_factor_type)
                        if success:
                            config = load_config()
                            config["auth_cookie"] = auth_cookie
                            config["user_id"] = user_id
                            config["last_login"] = datetime.now().isoformat()
                            config["remember_me"] = remember_me
                            
                            save_config(config)
                            messagebox.showinfo("Success", "Login successful!")
                            root.destroy()
                            import avatar_browser
                            avatar_browser.root.mainloop()
                        else:
                            messagebox.showerror("Error", "Invalid 2FA code")
                    
                    verify_button = ttk.Button(code_window, text="Verify", command=verify)
                    verify_button.pack(pady=10)
                    
                    code_window.protocol("WM_DELETE_WINDOW", lambda: code_window.destroy())
                    
                else:
                    # No 2FA required
                    config = load_config()
                    config["auth_cookie"] = auth_cookie
                    config["user_id"] = auth_response.json().get("id")
                    config["last_login"] = datetime.now().isoformat()
                    config["remember_me"] = remember_me
                    
                    save_config(config)
                    messagebox.showinfo("Success", "Login successful!")
                    root.destroy()

                    import avatar_browser
                    avatar_browser.root.mainloop()
                    
            else:
                messagebox.showerror("Error", f"Login failed: {auth_response.status_code}")
                
        except Exception as e:
            logging.error(f"Login error: {e}")
            messagebox.showerror("Error", f"Login failed: {str(e)}")

    # Create main window
    root = tk.Tk()
    root.title("VRChat Login")
    root.geometry("300x200")

    # Check if we have saved credentials and "Remember Me" is enabled
    config = load_config()
    if config.get("remember_me", False):
        try:
            headers = {"User-Agent": "VRChatAPI/1.0"}
            cookies = {"auth": config["auth_cookie"]}
            
            # Verify if the saved cookie is still valid
            verify_response = requests.get(
                f"{API_BASE}/auth/user",
                headers=headers,
                cookies=cookies
            )
            
            if verify_response.status_code == 200:
                # If valid, we're already logged in
                messagebox.showinfo("Success", "Already logged in!")
                root.destroy()
                import avatar_browser
                avatar_browser.root.mainloop()
                return
            elif verify_response.status_code == 401:
                # If unauthorized, clear the saved credentials
                config["auth_cookie"] = ""
                config["user_id"] = ""
                config["last_login"] = None
                save_config(config)
        except Exception as e:
            logging.error(f"Error verifying saved credentials: {e}")
            config["auth_cookie"] = ""
            config["user_id"] = ""
            config["last_login"] = None
            save_config(config)
    
    # Username
    ttk.Label(root, text="Username:").pack(pady=5)
    username_entry = ttk.Entry(root)
    username_entry.pack(pady=5)
    
    # Password
    ttk.Label(root, text="Password:").pack(pady=5)
    password_entry = ttk.Entry(root, show="*")
    password_entry.pack(pady=5)
    
    # Remember me
    remember_var = tk.BooleanVar()
    ttk.Checkbutton(root, text="Remember me", variable=remember_var).pack(pady=5)
    
    # Login button
    ttk.Button(root, text="Login", command=login).pack(pady=10)
    
    root.mainloop()

if __name__ == "__main__":
    login_window()
