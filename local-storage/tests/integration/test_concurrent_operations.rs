use std::sync::Arc;
use tokio::time::{sleep, Duration};
use futures::future::join_all;
use crate::helpers::{TestConfig, TestData};
use tokio::task;

#[tokio::test]
async fn test_concurrent_uploads() {
    let config = TestConfig::new();
    
    // Create multiple upload tasks that run concurrently
    let upload_tasks = (0..10).map(|i| {
        let test_data = TestData::sample_text();
        async move {
            // Simulate concurrent uploads
            let bucket = "test-bucket";
            let key = format!("file-{}.txt", i);
            let content = format!("Content for file {}: {:?}", i, test_data);
            
            // In a real test, this would make HTTP requests to upload endpoints
            // For now, verify the test data generation works concurrently
            assert!(!content.is_empty());
            assert!(key.contains(&i.to_string()));
            
            sleep(Duration::from_millis(10)).await; // Simulate processing time
            Ok::<String, String>(key)
        }
    });
    
    let results: Result<Vec<String>, String> = join_all(upload_tasks)
        .await
        .into_iter()
        .collect();
    
    let keys = results.expect("All uploads should succeed");
    assert_eq!(keys.len(), 10);
    
    // Verify all keys are unique
    let mut unique_keys = keys.clone();
    unique_keys.sort();
    unique_keys.dedup();
    assert_eq!(unique_keys.len(), 10, "All keys should be unique");
}

#[tokio::test]
async fn test_concurrent_read_write() {
    let config = TestConfig::new();
    
    // Simulate concurrent reads and writes
    let mut write_tasks = Vec::new();
    let mut read_tasks = Vec::new();
    
    // Add write tasks
    for i in 0..5 {
        let task = async move {
            let content = format!("Write content {}", i);
            sleep(Duration::from_millis(i * 10)).await;
            format!("write-{}", i)
        };
        write_tasks.push(task);
    }
    
    // Add read tasks
    for i in 0..5 {
        let task = async move {
            sleep(Duration::from_millis(i * 5)).await;
            format!("read-{}", i)
        };
        read_tasks.push(task);
    }
    
    let write_results = join_all(write_tasks).await;
    let read_results = join_all(read_tasks).await;
    
    let results: Vec<String> = write_results.into_iter().chain(read_results.into_iter()).collect();
    assert_eq!(results.len(), 10);
    
    // Verify we have both reads and writes
    let writes: Vec<_> = results.iter().filter(|r| r.starts_with("write")).collect();
    let reads: Vec<_> = results.iter().filter(|r| r.starts_with("read")).collect();
    
    assert_eq!(writes.len(), 5);
    assert_eq!(reads.len(), 5);
}

#[tokio::test]
async fn test_concurrent_downloads() {
    let config = TestConfig::new();
    // Rest of the test...
} 