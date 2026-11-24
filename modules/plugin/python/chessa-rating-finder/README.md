# Chessa Rating Finder

A Python-based Terminal User Interface (TUI) utility for searching player ratings on [CHESSA](https://ratings.chessa.co.za/).

## Requirements

- Python 3.10 or newer
- Internet connection
- Access to the CHESSA ratings website

## Installation

### Windows
Run `install_windows.bat`.

### Linux
Run:

```bash
bash install_linux.sh
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
