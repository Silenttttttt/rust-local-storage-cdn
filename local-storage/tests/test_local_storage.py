#!/usr/bin/env python3

import requests
import json
import os
from datetime import datetime
import hashlib
import blake3

# Configuration
API_URL = "http://192.168.1.218:30880"  # NodePort URL from README
TEST_BUCKET = "test-bucket"
TEST_FILE_CONTENT = b"Hello, this is a test file content!"
TEST_FILE_NAME = "test.txt"

def calculate_hashes(content):
    """Calculate MD5 and BLAKE3 hashes of content"""
    md5_hash = hashlib.md5(content).hexdigest()
    blake3_hash = blake3.blake3(content).hexdigest()
    return md5_hash, blake3_hash

def test_health():
    """Test health check endpoint"""
    print("\n🏥 Testing health check...")
    response = requests.get(f"{API_URL}/health")
    assert response.status_code == 200
    print("✅ Health check passed")

def test_storage_stats():
    """Test storage statistics endpoint"""
    print("\n📊 Testing storage stats...")
    response = requests.get(f"{API_URL}/stats")
    assert response.status_code == 200
    stats = response.json()
    print(f"📈 Current storage stats:")
    print(f"  • Total files: {stats.get('total_files', 0)}")
    print(f"  • Total size: {stats.get('total_size', 0)} bytes")
    print(f"  • Compressed files: {stats.get('compressed_files', 0)}")
    print(f"  • Encrypted files: {stats.get('encrypted_files', 0)}")

def test_file_operations():
    """Test file upload, download, and deletion"""
    print("\n📁 Testing file operations...")
    
    # Calculate file hashes
    md5_hash, blake3_hash = calculate_hashes(TEST_FILE_CONTENT)
    
    # Upload file
    print(f"📤 Uploading file to {TEST_BUCKET}/{TEST_FILE_NAME}...")
    files = {'file': ('test.txt', TEST_FILE_CONTENT, 'text/plain')}
    response = requests.post(f"{API_URL}/buckets/{TEST_BUCKET}/files", files=files)
    assert response.status_code == 201
    upload_info = response.json()
    print("✅ File uploaded successfully")
    print(f"  • File ID: {upload_info.get('id')}")
    print(f"  • Size: {upload_info.get('file_size')} bytes")
    print(f"  • MD5: {upload_info.get('hash_md5')}")
    print(f"  • BLAKE3: {upload_info.get('hash_blake3')}")
    
    # Verify hashes
    assert upload_info['hash_md5'] == md5_hash
    assert upload_info['hash_blake3'] == blake3_hash
    print("✅ File hashes verified")
    
    # Get file info
    print("\n📋 Getting file info...")
    response = requests.get(f"{API_URL}/buckets/{TEST_BUCKET}/files/{TEST_FILE_NAME}/info")
    assert response.status_code == 200
    file_info = response.json()
    print(f"  • Content Type: {file_info.get('content_type')}")
    print(f"  • Compressed: {file_info.get('is_compressed')}")
    print(f"  • Encrypted: {file_info.get('is_encrypted')}")
    
    # Download file
    print("\n📥 Downloading file...")
    response = requests.get(f"{API_URL}/buckets/{TEST_BUCKET}/files/{TEST_FILE_NAME}")
    assert response.status_code == 200
    assert response.content == TEST_FILE_CONTENT
    print("✅ File content verified")
    
    # List files in bucket
    print("\n📋 Listing files in bucket...")
    response = requests.get(f"{API_URL}/buckets/{TEST_BUCKET}/files")
    assert response.status_code == 200
    files = response.json()
    print(f"📁 Found {len(files)} files in bucket")
    for file in files:
        print(f"  • {file.get('key')} ({file.get('file_size')} bytes)")
    
    # Delete file
    print("\n🗑️ Deleting file...")
    response = requests.delete(f"{API_URL}/buckets/{TEST_BUCKET}/files/{TEST_FILE_NAME}")
    assert response.status_code == 204
    print("✅ File deleted successfully")

def test_bucket_operations():
    """Test bucket operations"""
    print("\n🗄️ Testing bucket operations...")
    
    # List buckets
    print("📋 Listing buckets...")
    response = requests.get(f"{API_URL}/buckets")
    assert response.status_code == 200
    buckets = response.json()
    print(f"Found {len(buckets)} buckets:")
    for bucket in buckets:
        print(f"  • {bucket}")
    
    # Get bucket stats
    print(f"\n📊 Getting stats for bucket {TEST_BUCKET}...")
    response = requests.get(f"{API_URL}/buckets/{TEST_BUCKET}/stats")
    if response.status_code == 200:
        stats = response.json()
        print(f"  • Files: {stats.get('file_count', 0)}")
        print(f"  • Total Size: {stats.get('total_size', 0)} bytes")

def main():
    """Run all tests"""
    print("🚀 Starting Local Storage API Tests")
    print(f"🌐 API URL: {API_URL}")
    
    try:
        test_health()
        test_storage_stats()
        test_bucket_operations()
        test_file_operations()
        print("\n✨ All tests completed successfully!")
        
    except AssertionError as e:
        print(f"\n❌ Test failed: {str(e)}")
    except requests.exceptions.RequestException as e:
        print(f"\n❌ Connection error: {str(e)}")
    except Exception as e:
        print(f"\n❌ Unexpected error: {str(e)}")

if __name__ == "__main__":
    main() 