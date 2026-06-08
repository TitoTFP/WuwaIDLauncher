import sys
import re
import os

def is_binary(file_path):
    try:
        with open(file_path, 'rb') as f:
            chunk = f.read(8000)
            return b'\x00' in chunk
    except Exception:
        return True

def main():
    args = sys.argv[1:]
    invert = False
    line_number = False
    pattern = None
    paths = []
    
    i = 0
    parse_opts = True
    while i < len(args):
        arg = args[i]
        if parse_opts and arg == '--':
            parse_opts = False
        elif parse_opts and arg == '-v':
            invert = True
        elif parse_opts and arg == '-n':
            line_number = True
        elif parse_opts and arg.startswith('-'):
            # Ignore other flags (e.g. -I, --no-progress)
            pass
        else:
            if pattern is None:
                pattern = arg
            else:
                paths.append(arg)
        i += 1
        
    if not pattern:
        print("Error: No pattern specified", file=sys.stderr)
        sys.exit(2)
        
    try:
        rx = re.compile(pattern)
    except Exception as e:
        print(f"Error compiling pattern {pattern}: {e}", file=sys.stderr)
        sys.exit(2)
        
    # If no paths, read stdin
    if not paths:
        matched = False
        for line_idx, line in enumerate(sys.stdin, 1):
            line = line.rstrip('\r\n')
            match = rx.search(line)
            if (match and not invert) or (not match and invert):
                matched = True
                if line_number:
                    print(f"{line_idx}:{line}")
                else:
                    print(line)
        sys.exit(0 if matched else 1)
        
    matched = False
    
    # Process paths
    files_to_scan = []
    for path in paths:
        if os.path.isdir(path):
            for root, dirs, files in os.walk(path):
                # skip git directories
                if '.git' in root.split(os.sep):
                    continue
                for file in files:
                    files_to_scan.append(os.path.join(root, file))
        elif os.path.isfile(path):
            files_to_scan.append(path)
            
    # If we have only 1 file, ripgrep by default does not print the filename prefix.
    # But if we have multiple files or we are scanning a directory, it prints the prefix.
    # Let's count how many files are actually being scanned.
    print_prefix = len(files_to_scan) > 1
    
    for file_path in files_to_scan:
        if is_binary(file_path):
            continue
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                for line_idx, line in enumerate(f, 1):
                    line = line.rstrip('\r\n')
                    match = rx.search(line)
                    if (match and not invert) or (not match and invert):
                        matched = True
                        prefix = f"{file_path}:" if print_prefix else ""
                        ln_prefix = f"{line_idx}:" if line_number else ""
                        print(f"{prefix}{ln_prefix}{line}")
        except Exception:
            pass
            
    sys.exit(0 if matched else 1)

if __name__ == "__main__":
    main()
