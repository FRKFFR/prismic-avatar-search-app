import tkinter as tk
from tkinter import ttk, messagebox
from concurrent.futures import ThreadPoolExecutor, as_completed
import requests
from PIL import Image, ImageTk, ImageDraw, ImageFont, UnidentifiedImageError
import io
import json
import threading
import webbrowser
import logging
import base64
import os

# Setup logging
logging.basicConfig(
    level=logging.DEBUG,
    format="%(asctime)s - %(levelname)s - %(message)s"
)

# Config
COLUMNS = 10
ROWS = 10
AVATARS_PER_PAGE = COLUMNS * ROWS

# Global variables for image storage
current_avatar_photo = None
current_avatar_error_photo = None

# --- Session State ---
session = {
    "auth_cookie": None,
    "twofa_cookie": None,
    "user_id": None
}
SESSION_FILE = "vrchat_session.json"

# Check if the session file exists
def get_auth_headers():
    cookies = f"auth={session['auth_cookie']}"
    if session.get("twofa_cookie"):
        cookies += f"; twoFactorAuth={session['twofa_cookie']}"
    return {
        "Cookie": cookies,
        "User-Agent": "VRChatAPI/1.0"
    }

# --- Login Window ---
def show_login_window():
    login_root = tk.Tk()
    login_root.title("VRChat Login")
    login_root.geometry("400x300")

    tk.Label(login_root, text="Email:").pack(pady=5)
    email_entry = tk.Entry(login_root)
    email_entry.pack(pady=5)

    tk.Label(login_root, text="Password:").pack(pady=5)
    password_entry = tk.Entry(login_root, show="*")
    password_entry.pack(pady=5)

    remember_var = tk.BooleanVar()
    remember_check = tk.Checkbutton(login_root, text="Remember Me", variable=remember_var)
    remember_check.pack(pady=5)

    def attempt_login():
        email = email_entry.get()
        password = password_entry.get()
        remember = remember_var.get()

        auth, user, needs_2fa = login_vrchat(email, password)

        if auth and user:
            session["auth_cookie"] = auth
            session["user_id"] = user
            save_session(email, password, remember)
            login_root.destroy()
            start_main_app()

        elif auth and needs_2fa:
            session["auth_cookie"] = auth
            login_root.withdraw()  # Hide the login window temporarily
            show_2fa_popup(email, password, remember)

        elif not auth:
            show_2fa_result(False)
            messagebox.showerror("Login Failed", "Invalid email or password.")

        else:
            show_2fa_result(False)
            messagebox.showerror("Login Failed", "Unexpected error.")


    login_button = tk.Button(login_root, text="Login", command=attempt_login)
    login_button.pack(pady=10)

    email_saved, pass_saved = load_saved_session()
    if email_saved and pass_saved:
        email_entry.insert(0, email_saved)
        password_entry.insert(0, pass_saved)
        remember_check.select()

    login_root.mainloop()

# Save session data to a file
def save_session(email, password, remember):
    if remember:
        with open(SESSION_FILE, 'w') as f:
            json.dump({"email": email, "password": password}, f)
    elif os.path.exists(SESSION_FILE):
        os.remove(SESSION_FILE)

# Load saved session
def load_saved_session():
    if os.path.exists(SESSION_FILE):
        with open(SESSION_FILE, 'r') as f:
            data = json.load(f)
            return data.get("email"), data.get("password")
    return None, None

# --- 2FA Popup ---
def show_2fa_popup(email, password, remember):
    def update_error_label(text):
        """Safe method to update error label that checks if widget exists"""
        try:
            error_label.config(text=text)
        except tk.TclError:
            # Widget has been destroyed, ignore error
            pass

    popup = tk.Toplevel()
    popup.grab_set()
    popup.title("Enter 2FA Code")
    popup.geometry("300x200")

    # Error label for displaying messages
    error_label = tk.Label(popup, text="", fg="red")
    error_label.pack(pady=5)

    label = tk.Label(popup, text="Enter your 2FA code from email or app:")
    label.pack(pady=10)

    code_entry = tk.Entry(popup, font=("Arial", 14), justify="center")
    code_entry.pack(pady=5)

    def on_submit():
        code = code_entry.get()
        auth_cookie = session.get("auth_cookie")
        methods = session.get("2fa_methods", [])

        headers = {
            "Content-Type": "application/json",
            "User-Agent": "VRChatAPI/1.0"
        }
        cookies = {"auth": auth_cookie}
        payload = {"code": code}

        if "emailOtp" in methods:
            logging.info("Using email OTP endpoint")
            endpoint = "https://api.vrchat.cloud/api/1/auth/twofactorauth/emailotp/verify"
        elif "totp" in methods:
            logging.info("Using TOTP (authenticator app) endpoint")
            endpoint = "https://api.vrchat.cloud/api/1/auth/twofactorauth/totp/verify"
        else:
            update_error_label("Error: No valid 2FA method found.")
            return

        try:
            response = requests.post(endpoint, headers=headers, cookies=cookies, json=payload)
            logging.debug(f"2FA response: {response.status_code}, {response.text}")
            if response.status_code == 200 and response.json().get("verified"):
                logging.info("2FA verified. Fetching user info...")

                # Fetch new cookies from response
                session["auth_cookie"] = response.cookies.get("auth") or session["auth_cookie"]
                session["twofa_cookie"] = response.cookies.get("twoFactorAuth")

                # Build cookie header with both
                full_cookies = {
                    "auth": session["auth_cookie"]
                }
                if session["twofa_cookie"]:
                    full_cookies["twoFactorAuth"] = session["twofa_cookie"]

                # Confirm user identity
                user_response = requests.get(
                    "https://api.vrchat.cloud/api/1/auth/user",
                    headers={"User-Agent": "VRChatAPI/1.0"},
                    cookies=full_cookies
                )
                if user_response.status_code == 200:
                    user_data = user_response.json()
                    session["user_id"] = user_data.get("id")
                    logging.info(f"Login complete. User ID: {session['user_id']}")
                    popup.destroy()
                    start_main_app()
                else:
                    update_error_label("Error: Failed to retrieve user data.")
            else:
                update_error_label("Error: Invalid or expired 2FA code.")
        except Exception as e:
            logging.error(f"Error verifying 2FA code: {e}")
            update_error_label(f"Error: Unexpected error: {str(e)}")

    submit_btn = tk.Button(popup, text="Submit", command=on_submit)
    submit_btn.pack(pady=10)

    # Wait for user input
    popup.wait_window()

# --- 2FA Verification Topt---
def verify_2fa_code(code, auth_cookie):
    try:
        cookies = {'auth': auth_cookie}
        headers = {
            "Content-Type": "application/json",
            "User-Agent": "VRChatAPI/1.0"
        }
        payload = {"code": code}

        response = requests.post(
            "https://api.vrchat.cloud/api/1/auth/twofactorauth/totp/verify",
            headers=headers,
            cookies=cookies,
            json=payload
        )

        if response.status_code == 200:
            data = response.json()
            if data.get("verified") is True:
                logging.info("2FA code verified successfully.")

                # Optionally: store twoFactorAuth cookie for future requests
                two_factor_cookie = response.cookies.get('twoFactorAuth')
                if two_factor_cookie:
                    logging.info(f"Received twoFactorAuth cookie: {two_factor_cookie[:10]}...")

                # Confirm user is fully authenticated now
                verify = requests.get(
                    "https://api.vrchat.cloud/api/1/auth/user",
                    headers={"User-Agent": "VRChatAPI/1.0"},
                    cookies={**cookies, 'twoFactorAuth': two_factor_cookie} if two_factor_cookie else cookies
                )

                if verify.status_code == 200:
                    session["user_id"] = verify.json().get("id")
                    return True
                else:
                    logging.error(f"Failed to verify user after 2FA: {verify.status_code}")
                    return False
            else:
                logging.warning("2FA code not verified.")
                return False
        else:
            logging.error(f"2FA verification failed: {response.status_code}, {response.text}")
            return False

        return False
    except Exception as e:
        logging.error(f"Error verifying 2FA code: {e}")
        return False

# --- 2FA Verification email---
def verify_email_2fa(code, auth_cookie):
    try:
        session_req = requests.Session()
        session_req.headers.update({
            "Content-Type": "application/json",
            "User-Agent": "VRChatAPI/1.0"
        })
        session_req.cookies.set("auth", auth_cookie)

        response = session_req.post(
            "https://api.vrchat.cloud/api/1/auth/twofactorauth/emailotp/verify",
            json={"code": code}
        )

        if response.status_code == 200 and response.json().get("verified"):
            logging.info("Email-based 2FA verified successfully.")

            two_factor_cookie = response.cookies.get("twoFactorAuth")
            if two_factor_cookie:
                logging.info(f"Received twoFactorAuth cookie: {two_factor_cookie[:10]}...")
            return two_factor_cookie

        else:
            logging.error(f"Email 2FA failed: {response.status_code} - {response.text}")
            return None
    except Exception as e:
        logging.error(f"Exception during email 2FA verification: {e}")
        return None

# --- Login Function ---
def login_vrchat(email, password, twofa_code=None):
    logging.info("Attempting to log in to VRChat")
    session_req = requests.Session()
    session_req.headers.update({"User-Agent": "VRChatAPI/1.0"})
    credentials = base64.b64encode(f"{email}:{password}".encode()).decode()
    session_req.headers["Authorization"] = f"Basic {credentials}"

    # Initial login attempt
    response = session_req.get("https://api.vrchat.cloud/api/1/auth/user")
    auth_cookie = session_req.cookies.get("auth")
    data = response.json()
    logging.debug(f"Login response: {response.status_code}, {data}")
    session["2fa_methods"] = data.get("requiresTwoFactorAuth", [])


    auth_cookie = session_req.cookies.get("auth")

    # Successful login without 2FA required
    if response.status_code == 200:
        data = response.json()
        if data.get("requiresTwoFactorAuth"):
            logging.info("2FA required. Awaiting user code input.")
            return auth_cookie, None, True
        elif data.get("id"):
            user_id = data["id"]
            logging.info(f"Login successful. User ID: {user_id}")
            return auth_cookie, user_id, False
        else:
            logging.error("Login returned 200 but no user ID found. Unexpected response.")
            return None, None, False


    # 2FA required but no code provided yet
    elif response.status_code == 401 and not twofa_code:
        logging.info("2FA required. Awaiting user code input.")
        return auth_cookie, None, True

    # 2FA code provided, try to verify it
    elif twofa_code and auth_cookie:
        # Try TOTP first
        response_totp = session_req.post(
            "https://api.vrchat.cloud/api/1/auth/twofactorauth/totp/verify",
            headers={"Content-Type": "application/json"},
            cookies={"auth": auth_cookie},
            json={"code": twofa_code}
        )
        logging.debug(f"TOTP 2FA response: {response_totp.status_code}, {response_totp.text}")

        if response_totp.status_code == 200 and response_totp.json().get("verified"):
            logging.info("TOTP 2FA verified successfully.")
            two_factor_cookie = response_totp.cookies.get("twoFactorAuth")
        else:
            # Fallback: try email 2FA
            response_email = session_req.post(
                "https://api.vrchat.cloud/api/1/auth/twofactorauth/emailotp/verify",
                headers={"Content-Type": "application/json"},
                cookies={"auth": auth_cookie},
                json={"code": twofa_code}
            )
            logging.debug(f"Email 2FA response: {response_email.status_code}, {response_email.text}")

            if response_email.status_code == 200 and response_email.json().get("verified"):
                logging.info("Email 2FA verified successfully.")
                two_factor_cookie = response_email.cookies.get("twoFactorAuth")
            else:
                logging.error("2FA verification failed (TOTP and Email).")
                return None, None, False

        # Confirm login with both cookies
        confirm = session_req.get(
            "https://api.vrchat.cloud/api/1/auth/user",
            cookies={"auth": auth_cookie, "twoFactorAuth": two_factor_cookie}
        )
        if confirm.status_code == 200:
            user_id = confirm.json().get("id")
            logging.info(f"2FA complete. User ID: {user_id}")
            return auth_cookie, user_id, False
        else:
            logging.error("2FA passed but failed to fetch user info.")
            return None, None, False

    else:
        logging.error(f"Login failed: {response.status_code}")
        return None, None, False

# --- Application Launcher ---
def start_main_app():
    global auth_cookie, user_id, root, current_page, filtered_avatars, avatar_widgets, banned_avatars_count, loading_label, progress_var_avatars, progress_var_images, banned_count_label
    auth_cookie = session["auth_cookie"]
    user_id = session["user_id"]
    # The rest of your application setup and GUI goes here.
    # You would move your original GUI logic (Tk, filters, canvas, avatar grid, etc.) into this function

    # --- Main Application Logic ---
    # Load avatars
    with open('Avatar Data.json', 'r', encoding='utf-8') as f:
        avatars_data = json.load(f)
        logging.info(f"Loaded {len(avatars_data)} avatars.")

    # Globals
    filtered_avatars = avatars_data
    current_page = 0
    avatar_widgets = []
    banned_avatars_count = 0  # Counter for banned/deleted avatars

    # Tkinter setup
    root = tk.Tk()
    root.title("VRChat Avatar Browser Prismic database By FR_KF_FR")
    root.geometry("1800x1000")

    # Create frames
    filter_frame = tk.Frame(root)
    filter_frame.pack(pady=10)

    # --- [ADDED] ---
    # Frame for Current Avatar Info (inside filter frame)
    current_avatar_frame = tk.Frame(filter_frame)
    current_avatar_frame.grid(row=0, column=0, rowspan=2, padx=10, pady=5, sticky="w")

    # Create empty label first
    current_avatar_img_label = tk.Label(current_avatar_frame)
    current_avatar_img_label.pack(side="top", pady=5)

    current_avatar_name_label = tk.Label(current_avatar_frame, text="Current Avatar", font=("Arial", 12, "bold"), wraplength=100)
    current_avatar_name_label.pack(side="top")

    # Create default images after everything else is initialized
    def create_default_images():
        try:
            # Create a default loading image
            loading_img = Image.new('RGBA', (100, 100), (255, 255, 255, 255))
            draw = ImageDraw.Draw(loading_img)
            font = ImageFont.truetype("arial.ttf", 24)
            draw.text((15, 35), "Loading...", font=font, fill=(0, 0, 0, 255))
            tk_default_img = ImageTk.PhotoImage(loading_img)

            # Store the image reference on the label
            current_avatar_img_label._loading_image = tk_default_img
            
            # Update the label with the image
            current_avatar_img_label.config(image=tk_default_img)

            # Store the image reference on the root window
            root._loading_image = tk_default_img

        except Exception as e:
            logging.error(f"Error creating default images: {e}")
            # Fallback to simple text if image creation fails
            current_avatar_img_label.config(text="Loading...")

    # Schedule image creation after the main window is ready
    root.after(100, create_default_images)

    # Load current avatar
    def load_current_avatar():
        def _load():
            try:
                auth_cookie = session.get("auth_cookie")
                if not auth_cookie:
                    return None

                headers = {"User-Agent": "VRChatAPI/1.0"}
                cookies = {"auth": auth_cookie}
                
                # Get current avatar data
                avatar_response = requests.get(
                    f"https://api.vrchat.cloud/api/1/users/{session['user_id']}/avatar",
                    headers=headers,
                    cookies=cookies
                )
                
                if avatar_response.status_code != 200:
                    logging.error(f"Failed to fetch current avatar: {avatar_response.status_code}")
                    return None

                avatar_data = avatar_response.json()
                if not avatar_data.get("imageId"):
                    logging.error("Current avatar has no imageId")
                    return None

                # Get avatar image
                image_response = requests.get(
                    f"https://api.vrchat.cloud/api/1/file/{avatar_data['imageId']}/3/file",
                    headers=headers,
                    cookies=cookies
                )

                if image_response.status_code != 200:
                    logging.error(f"Failed to fetch avatar image: {image_response.status_code}")
                    return None

                # Process image in background thread
                img = Image.open(io.BytesIO(image_response.content)).convert("RGBA")
                img = img.resize((100, 100), Image.LANCZOS)

                # Convert to PNG bytes
                with io.BytesIO() as output:
                    img.save(output, format="PNG")
                    processed_img_data = output.getvalue()

                # Create and update image on main thread
                def update_image():
                    try:
                        img = Image.open(io.BytesIO(processed_img_data)).convert("RGBA")
                        tk_img = ImageTk.PhotoImage(img)
                        current_avatar_img_label._avatar_image = tk_img  # Store reference
                        current_avatar_img_label.config(image=tk_img)
                        current_avatar_name_label.config(text=avatar_data['name'])
                    except Exception as e:
                        logging.error(f"Error updating avatar image: {e}")
                        # Show error image if available
                        if hasattr(root, '_loading_image'):
                            current_avatar_img_label.config(image=root._loading_image)
                        else:
                            current_avatar_img_label.config(text="Error")

                root.after(0, update_image)

            except Exception as e:
                logging.error(f"Error loading current avatar: {e}")
                # Show error image if available
                if hasattr(root, '_loading_image'):
                    current_avatar_img_label.config(image=root._loading_image)
                else:
                    current_avatar_img_label.config(text="Error")

        threading.Thread(target=_load).start()

    # Schedule current avatar loading after images are initialized
    root.after(200, load_current_avatar)

    # --- [END ADD] ---
    # --- [END ADD] ---

    # Name/Description Search
    search_var = tk.StringVar()
    search_entry = tk.Entry(filter_frame, textvariable=search_var, font=("Arial", 14), width=50)
    search_label = tk.Label(filter_frame, text="Search Name/Description:", font=("Arial", 12))

    # Author Search
    author_var = tk.StringVar()
    author_entry = tk.Entry(filter_frame, textvariable=author_var, font=("Arial", 14), width=50)
    author_label = tk.Label(filter_frame, text="Search Author:", font=("Arial", 12))

    # Platform filter checkboxes
    platforms_var = {"PC": tk.BooleanVar(), "Quest": tk.BooleanVar(), "iOS": tk.BooleanVar()}
    platforms_frame = tk.LabelFrame(filter_frame, text="Filter by Platforms", font=("Arial", 12), padx=10, pady=10)

    pc_checkbox = tk.Checkbutton(platforms_frame, text="PC", variable=platforms_var["PC"])
    quest_checkbox = tk.Checkbutton(platforms_frame, text="Quest", variable=platforms_var["Quest"])
    ios_checkbox = tk.Checkbutton(platforms_frame, text="iOS", variable=platforms_var["iOS"])

    # Page navigation buttons
    page_nav_frame = tk.Frame(filter_frame)
    prev_button = tk.Button(page_nav_frame, text="Previous", command=lambda: change_page(-1))
    prev_button.grid(row=0, column=0, padx=5)

    page_label = tk.Label(page_nav_frame, text="Page 1")
    page_label.grid(row=0, column=1, padx=5)

    next_button = tk.Button(page_nav_frame, text="Next", command=lambda: change_page(1))
    next_button.grid(row=0, column=2, padx=5)

    # --- [SHIFT EVERYTHING RIGHT BY 1 COLUMN] ---
    search_label.grid(row=0, column=1, padx=5, pady=5)
    search_entry.grid(row=1, column=1, padx=5, pady=5)
    author_label.grid(row=0, column=2, padx=5, pady=5)
    author_entry.grid(row=1, column=2, padx=5, pady=5)
    platforms_frame.grid(row=0, column=3, rowspan=2, padx=5, pady=5)
    pc_checkbox.grid(row=0, column=0, padx=10, pady=5)
    quest_checkbox.grid(row=0, column=1, padx=10, pady=5)
    ios_checkbox.grid(row=0, column=2, padx=10, pady=5)
    page_nav_frame.grid(row=0, column=4, rowspan=2, padx=5, pady=5)

    # Search Button
    search_button = tk.Button(filter_frame, text="Search", command=lambda: filter_avatars(0))
    search_button.grid(row=2, column=0, columnspan=5, pady=10)

    # Scrollable frame
    canvas = tk.Canvas(root)
    scrollbar = ttk.Scrollbar(root, orient="vertical", command=canvas.yview)
    scrollable_frame = ttk.Frame(canvas)

    scrollable_frame.bind(
        "<Configure>",
        lambda e: canvas.configure(scrollregion=canvas.bbox("all"))
    )

    canvas.create_window((0, 0), window=scrollable_frame, anchor="nw")
    canvas.configure(yscrollcommand=scrollbar.set)
    canvas.pack(side="left", fill="both", expand=True)
    scrollbar.pack(side="right", fill="y")

    # Enable mouse scrolling
    def on_mouse_wheel(event):
        canvas.yview_scroll(int(-1*(event.delta/120)), "units")
        canvas.update_idletasks()

    canvas.bind_all("<MouseWheel>", on_mouse_wheel)

    # Clear widgets
    def clear_frame():
        global avatar_widgets
        for widget in avatar_widgets:
            widget.destroy()
        avatar_widgets = []

    # Fetch current avatar
    def fetch_current_avatar(auth_cookie):
        try:
            logging.debug(f"Fetching currently equipped avatar for user {user_id}")
            r = requests.get(
                f"https://api.vrchat.cloud/api/1/users/{user_id}/avatar",
                headers={"Cookie": f"auth={auth_cookie}", "User-Agent": "VRChatAPI/1.0"}
            )
            if r.status_code == 200:
                return r.json()
            else:
                logging.error(f"Failed to fetch current avatar: Status {r.status_code}")
                return None
        except Exception as e:
            logging.error(f"Error fetching current avatar: {e}")
            return None

    # Show 2FA verification result
    def show_2fa_result(success):
        if success:
            messagebox.showinfo("Success", "2FA verification successful!")
        else:
            messagebox.showerror("Error", "2FA verification failed. Please try again.")

    # Load current avatar
    def load_current_avatar():
        # Store image references at class level
        current_avatar_img_label._photo = None
        current_avatar_img_label._error_photo = None

        def _load():
            try:
                headers = {
                    "Cookie": f"auth={session['auth_cookie']}; twoFactorAuth={session.get('twofa_cookie', '')}",
                    "User-Agent": "VRChat/2024.1.2"
                }
                user_response = requests.get("https://api.vrchat.cloud/api/1/auth/user", headers=headers)
                user_response.raise_for_status()
                user_data = user_response.json()

                current_avatar_id = user_data.get('currentAvatar')
                if not current_avatar_id:
                    logging.warning("No current avatar found in user data.")
                    return

                avatar_response = requests.get(
                    f"https://api.vrchat.cloud/api/1/avatars/{current_avatar_id}", headers=headers
                )
                avatar_response.raise_for_status()
                avatar_data = avatar_response.json()

                image_url = avatar_data.get('imageUrl') or avatar_data.get('thumbnailImageUrl')
                if not image_url:
                    logging.warning("No image URL for current avatar.")
                    return

                img_response = requests.get(image_url, headers=headers)
                img_data = img_response.content

                # Process image data in background thread
                try:
                    # Process image completely in background thread
                    img = Image.open(io.BytesIO(img_data)).convert("RGBA")
                    img = img.resize((100, 100), Image.LANCZOS)
                    
                    # Convert image to bytes in background thread
                    with io.BytesIO() as output:
                        img.save(output, format="PNG")
                        processed_img_data = output.getvalue()

                    # Schedule UI update on main thread with processed image data
                    def apply_image():
                        try:
                            # Create image from processed data on main thread
                            img = Image.open(io.BytesIO(processed_img_data)).convert("RGBA")
                            tk_img = ImageTk.PhotoImage(img)
                            # Store the image reference at class level
                            current_avatar_img_label._photo = tk_img
                            current_avatar_img_label.config(image=tk_img)
                            current_avatar_name_label.config(text=avatar_data['name'])
                            logging.debug("Avatar image loaded and displayed.")
                        except Exception as e:
                            logging.error(f"UI update failed (apply_image): {e}")
                            # Create error image on main thread
                            error_img = Image.new('RGBA', (100, 100), color='red')
                            error_text = ImageDraw.Draw(error_img)
                            error_text.text((10, 45), "Error", fill="white")
                            tk_error_img = ImageTk.PhotoImage(error_img)
                            current_avatar_img_label._photo = tk_error_img
                            current_avatar_img_label.config(image=tk_error_img)

                    root.after(0, apply_image)  # Schedule on UI thread

                except Exception as e:
                    logging.error(f"Image processing error: {e}")
                    # Create error image on main thread
                    def update_error():
                        error_img = Image.new('RGBA', (100, 100), color='red')
                        error_text = ImageDraw.Draw(error_img)
                        error_text.text((10, 45), "Error", fill="white")
                        tk_error_img = ImageTk.PhotoImage(error_img)
                        current_avatar_img_label._photo = tk_error_img
                        current_avatar_img_label.config(image=tk_error_img)
                    root.after(0, update_error)

            except Exception as e:
                logging.error(f"Failed to load current avatar: {e}")
                # Create error image on main thread
                def update_error():
                    error_img = Image.new('RGBA', (100, 100), color='red')
                    error_text = ImageDraw.Draw(error_img)
                    error_text.text((10, 45), "Error", fill="white")
                    tk_error_img = ImageTk.PhotoImage(error_img)
                    current_avatar_img_label._photo = tk_error_img
                    current_avatar_img_label.config(image=tk_error_img)
                root.after(0, update_error)

        threading.Thread(target=_load, daemon=True).start()

    # Fetch avatar details
    def fetch_avatar_details(avatar_id):
        global banned_avatars_count
        try:
            logging.debug(f"Fetching details for avatar {avatar_id}")
            r = requests.get(f"https://api.vrchat.cloud/api/1/avatars/{avatar_id}",
                            headers={"Cookie": f"auth={auth_cookie}", "User-Agent": "VRChatAPI/1.0"})
            if r.status_code == 200:
                return r.json()
            elif r.status_code == 404:  # Banned or deleted avatar
                banned_avatars_count += 1
                banned_count_label.config(text=f"Banned Avatars: {banned_avatars_count}")
                return None
            else:
                logging.error(f"Failed to fetch avatar {avatar_id}: Status {r.status_code}")
        except Exception as e:
            logging.error(f"Error fetching {avatar_id}: {e}")
        return None

    # Fetch avatar image
    def fetch_avatar_image(image_url, platforms):
        try:
            logging.debug(f"Fetching image {image_url}")
            
            # Add timeout to prevent hanging
            img_response = requests.get(image_url, 
                                     headers={"Cookie": f"auth={auth_cookie}", "User-Agent": "VRChatAPI/1.0"},
                                     timeout=10)
            
            if img_response.status_code != 200:
                logging.error(f"Failed to fetch image {image_url}: Status {img_response.status_code}")
                messagebox.showerror("Error", f"Failed to load image: Server returned status {img_response.status_code}")
                return None
                
            img_data = img_response.content
            if not img_data:
                logging.error(f"Empty response for image {image_url}")
                messagebox.showerror("Error", "Received empty image data")
                return None

            img = Image.open(io.BytesIO(img_data)).convert("RGBA")
            img = img.resize((120, 120))

            # Draw platform text
            draw = ImageDraw.Draw(img)
            font = ImageFont.load_default()
            platform_colors = {
                "PC": "blue",
                "Quest": "green",
                "iOS": "purple"
            }
            y = 2
            for platform in platforms:
                text = platform
                color = platform_colors.get(platform, "white")
                bbox = draw.textbbox((0, 0), text, font=font)
                w, h = bbox[2] - bbox[0], bbox[3] - bbox[1]
                draw.rectangle([120-w-8, y, 120-2, y+h+2], fill="black")
                draw.text((120-w-5, y), text, font=font, fill=color)
                y += h + 4

            return ImageTk.PhotoImage(img)
        except requests.exceptions.Timeout:
            logging.error(f"Timeout while fetching image {image_url}")
            messagebox.showerror("Error", "Timeout while loading image. Please check your internet connection.")
            return None
        except (UnidentifiedImageError, Exception) as e:
            logging.error(f"Error loading image {image_url}: {e}")
            messagebox.showerror("Error", f"Failed to load image: {str(e)}")
            return None

    # Show avatar info
    def show_info(avatar):
        info = f"Name: {avatar['name']}\nAuthor: {avatar['author']}\nDescription: {avatar['description']}"
        messagebox.showinfo("Avatar Info", info)

    # Open avatar page in web browser
    def open_avatar_page(avatar_id):
        url = f"https://vrchat.com/home/avatar/{avatar_id}"
        webbrowser.open(url)

    # Filter avatars based on search and platform
    def filter_avatars(page):
        global filtered_avatars, current_page
        name_desc_query = search_var.get().lower()
        author_query = author_var.get().lower()

        logging.debug(f"Filtering avatars with Name/Description '{name_desc_query}' and Author '{author_query}'")

        filtered_avatars = [
            avatar for avatar in avatars_data
            if (name_desc_query in avatar['name'].lower() or name_desc_query in avatar['description'].lower())
            and (author_query in avatar['author'].lower())
        ]

        selected_platforms = [platform for platform, var in platforms_var.items() if var.get()]
        if selected_platforms:
            filtered_avatars = [
                avatar for avatar in filtered_avatars
                if any(platform in selected_platforms for platform in avatar['platforms'])
            ]

        logging.debug(f"{len(filtered_avatars)} avatars matched the filters.")

        current_page = page
        threaded_display_avatars(current_page)

    # Define the function to handle selecting the avatar
    def select_avatar(avatar_id):
        url = f"https://api.vrchat.cloud/api/1/avatars/{avatar_id}/select"
        headers = {
            "Cookie": f"auth={auth_cookie}",
            "User-Agent": "VRChatAPI/1.0"
        }
        try:
            # Send the PUT request to select the avatar
            response = requests.put(url, headers=headers)
            
            if response.status_code == 200:
                logging.info(f"Avatar {avatar_id} selected successfully.")
                messagebox.showinfo("Success", f"Avatar {avatar_id} selected successfully!")
                
                # 🟰 Refresh the current avatar display
                load_current_avatar()

            else:
                logging.error(f"Failed to select avatar {avatar_id}: {response.status_code}")
                messagebox.showerror("Error", f"Failed to select avatar {avatar_id}. Status: {response.status_code}")

        except Exception as e:
            logging.error(f"Error selecting avatar {avatar_id}: {e}")
            messagebox.showerror("Error", f"Error selecting avatar {avatar_id}: {str(e)}")

    # Update the "display_avatars" function to include the Select button
    def display_avatars(page):
        logging.debug(f"Displaying avatars for page {page + 1}")
        clear_frame()

        start = page * AVATARS_PER_PAGE
        end = start + AVATARS_PER_PAGE
        avatars_to_display = filtered_avatars[start:end]

        row = 0
        col = 0

        futures = []
        total_avatars = len(avatars_to_display)

        with ThreadPoolExecutor(max_workers=10) as executor:
            details_futures = {executor.submit(fetch_avatar_details, avatar['avatar_id']): avatar for avatar in avatars_to_display}
            
            for future in as_completed(details_futures):
                avatar = details_futures[future]
                details = future.result()

                if details:
                    # Get the avatar image URL
                    image_url = details.get('imageUrl') or details.get('thumbnailImageUrl')
                    if not image_url:
                        continue

                    # Fetch the image in parallel
                    img_future = executor.submit(fetch_avatar_image, image_url, avatar['platforms'])
                    img = img_future.result()  # Get the image when ready

                    # Create a new avatar container widget and show image immediately
                    container = tk.Frame(scrollable_frame, bd=2, relief=tk.RIDGE, width=180, height=270)
                    container.grid(row=row, column=col, padx=5, pady=5)
                    container.grid_propagate(False)

                    avatar_label = tk.Label(container, image=img)
                    avatar_label.image = img
                    avatar_label.pack(pady=5)

                    name_label = tk.Label(container, text=avatar['name'], font=("Arial", 10, "bold"), wraplength=160)
                    name_label.pack()

                    description_label = tk.Label(container, text=avatar['description'], font=("Arial", 8),
                                                wraplength=160, justify="left")
                    description_label.pack(pady=3)

                    buttons_frame = tk.Frame(container)
                    buttons_frame.pack()

                    info_button = tk.Button(buttons_frame, text="?", width=2, command=lambda a=avatar: show_info(a))
                    info_button.pack(side="left", padx=5)

                    select_button = tk.Button(buttons_frame, text="Open Web", command=lambda id=avatar['avatar_id']: open_avatar_page(id))
                    select_button.pack(side="right", padx=5)

                    # Add the Select button
                    select_button = tk.Button(buttons_frame, text="Select", command=lambda id=avatar['avatar_id']: select_avatar(id))
                    select_button.pack(side="right", padx=5)

                    avatar_widgets.append(container)

                    col += 1
                    if col >= COLUMNS:
                        col = 0
                        row += 1

                    # Update progress bars
                    progress_var_avatars.set(((len(futures) + 1) / total_avatars) * 100)
                    progress_bar_avatars.update()

                    progress_var_images.set(((len(futures) + 1) / total_avatars) * 100)
                    progress_bar_images.update()

                    # Force Tkinter to refresh the UI
                    root.after(10)

        # Hide loading bars when done
        loading_label.place_forget()
        progress_bar_avatars.place_forget()
        progress_bar_images.place_forget()

        page_label.config(text=f"Page {current_page + 1} / {max(1, (len(filtered_avatars) + AVATARS_PER_PAGE - 1) // AVATARS_PER_PAGE)}")

    # Threaded display function
    def threaded_display_avatars(page):
        loading_label.pack(side="left", padx=10, pady=5)
        progress_bar_avatars.pack(side="left", padx=10, pady=5)
        progress_bar_images.pack(side="left", padx=10, pady=5)

        # Start thread for displaying avatars
        threading.Thread(target=lambda: display_avatars(page), daemon=True).start()

    # Change page function
    def change_page(direction):
        global current_page
        new_page = current_page + direction
        if 0 <= new_page < len(filtered_avatars) // AVATARS_PER_PAGE + 1:
            current_page = new_page
            threaded_display_avatars(current_page)

    load_current_avatar()
    root.mainloop()

show_login_window()
