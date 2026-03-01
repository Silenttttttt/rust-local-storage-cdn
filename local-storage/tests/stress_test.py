#!/usr/bin/env python3

import requests
import json
import os
import time
import random
import string
import concurrent.futures
import humanize
from datetime import datetime
from pathlib import Path
import statistics

# Configuration
API_URL = "http://192.168.1.218:30880"  # NodePort URL from README
STRESS_TEST_BUCKET = "stress-test-bucket"
CONCURRENT_OPERATIONS = 5
FILE_SIZES = [
    (1024, "1KB"),
    (1024 * 1024, "1MB"),
    (5 * 1024 * 1024, "5MB"),
]
ITERATIONS_PER_SIZE = 3

def generate_random_content(size):
    """Generate random file content of specified size"""
    return os.urandom(size)

def generate_random_filename():
    """Generate a random filename"""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    random_str = ''.join(random.choices(string.ascii_letters + string.digits, k=8))
    return f"test_{timestamp}_{random_str}.bin"

def ensure_bucket_exists():
    """Create the stress test bucket if it doesn't exist"""
    try:
        # Try to list files in the bucket to check if it exists
        response = requests.get(f"{API_URL}/buckets/{STRESS_TEST_BUCKET}/files")
        if response.status_code == 404:
            # Create bucket by uploading a test file
            test_file = {"file": ("test.txt", b"test")}
            response = requests.post(f"{API_URL}/buckets/{STRESS_TEST_BUCKET}/files", files=test_file)
            if response.status_code not in [200, 201]:
                print(f"❌ Failed to create bucket: {response.status_code}")
                try:
                    print(f"Error response: {response.json()}")
                except:
                    print(f"Raw response: {response.text}")
                return False
            print(f"✅ Created bucket {STRESS_TEST_BUCKET}")
        return True
    except Exception as e:
        print(f"❌ Error ensuring bucket exists: {e}")
        return False

def upload_file(size):
    """Upload a file of specified size"""
    try:
        content = generate_random_content(size)
        filename = generate_random_filename()
        files = {"file": (filename, content)}
        
        start_time = time.time()
        response = requests.post(f"{API_URL}/buckets/{STRESS_TEST_BUCKET}/files", files=files)
        duration = time.time() - start_time
        
        if response.status_code not in [200, 201]:
            print(f"❌ Upload failed for {filename} ({humanize.naturalsize(size)})")
            try:
                print(f"Error response: {response.json()}")
            except:
                print(f"Raw response: {response.text}")
            return None
        
        return {
            "size": size,
            "duration": duration,
            "status": response.status_code
        }
    except Exception as e:
        print(f"❌ Error during upload: {e}")
        return None

def run_stress_test():
    """Run the stress test"""
    print("\n🚀 Starting Local Storage Stress Test")
    
    if not ensure_bucket_exists():
        print("❌ Failed to ensure bucket exists. Aborting test.")
        return
    
    results = []
    
    for size, size_label in FILE_SIZES:
        print(f"\n📦 Testing {size_label} files")
        size_results = []
        
        for iteration in range(ITERATIONS_PER_SIZE):
            print(f"\n🔄 Iteration {iteration + 1}/{ITERATIONS_PER_SIZE}")
            
            with concurrent.futures.ThreadPoolExecutor(max_workers=CONCURRENT_OPERATIONS) as executor:
                futures = [executor.submit(upload_file, size) for _ in range(CONCURRENT_OPERATIONS)]
                iteration_results = [f.result() for f in futures]
                
                # Filter out failed uploads
                iteration_results = [r for r in iteration_results if r is not None]
                size_results.extend(iteration_results)
        
        if size_results:
            durations = [r["duration"] for r in size_results]
            avg_duration = statistics.mean(durations)
            throughput = size * len(size_results) / sum(durations)
            
            print(f"\n📊 Results for {size_label}:")
            print(f"  - Average upload time: {avg_duration:.2f} seconds")
            print(f"  - Throughput: {humanize.naturalsize(throughput)}/second")
            print(f"  - Success rate: {len(size_results)}/{ITERATIONS_PER_SIZE * CONCURRENT_OPERATIONS}")
            
            results.append({
                "size": size,
                "size_label": size_label,
                "avg_duration": avg_duration,
                "throughput": throughput,
                "success_rate": len(size_results)/(ITERATIONS_PER_SIZE * CONCURRENT_OPERATIONS)
            })
    
    return results

if __name__ == "__main__":
    run_stress_test() 