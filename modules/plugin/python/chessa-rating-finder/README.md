# Chessa Rating Finder

A Python-based Terminal User Interface (TUI) utility for searching player ratings on [CHESSA](https://ratings.chessa.co.za/).

## Requirements

- Python 3.10 or newer
- Internet connection
- Access to the CHESSA ratings website

## Installation

**1. Open a terminal**

Navigate to the project directory:

```powershell
cd "C:\Users\YourName\path\to\chessa-rating-finder"
```

Replace the example path with the actual location of your project.

**2. Create a virtual environment (recommended)**

```powershell
python -m venv .venv
```

**3. Activate the virtual environment**

For Windows PowerShell:

```powershell
.\.venv\Scripts\Activate.ps1
```

If PowerShell blocks script execution, you can use:

```powershell
.\.venv\Scripts\python.exe -m pip install -r requirements.txt
```

And use that same Python executable to run the application.

**4. Install dependencies**

With the virtual environment activated:

```powershell
python -m pip install -r requirements.txt
```

## Running the Application

Start the TUI using:

```powershell
python chessa_rating_tui.py
```

The application will launch in your terminal.

## Running Tests

To run the automated tests:

```powershell
python -m pytest -v
```

## Project Structure

```text
chessa-rating-finder/
│
├── chessa_rating_tui.py
├── requirements.txt
├── test_chessa_rating_tui.py
└── README.md
```

## Usage

The utility searches the CHESSA ratings database using player information, including:

- Surname (primary search criterion)
- First name
- Date of birth

The matching player's rating information is retrieved and displayed in the terminal interface.

## Notes

- An active internet connection is required.
- Search results depend on the availability and accessibility of the CHESSA website.
- Keep dependencies up to date using `requirements.txt`.
