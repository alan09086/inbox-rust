use inboxly_imap::pool::PoolConfig;
use std::time::Duration;

#[test]
fn pool_config_defaults() {
    let config = PoolConfig::default();
    assert_eq!(config.max_connections, 3);
    assert_eq!(config.connect_timeout, Duration::from_secs(30));
    assert_eq!(config.idle_timeout, Duration::from_secs(300));
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_base_delay, Duration::from_secs(1));
}

#[test]
fn pool_config_custom() {
    let config = PoolConfig {
        max_connections: 5,
        connect_timeout: Duration::from_secs(60),
        idle_timeout: Duration::from_secs(600),
        max_retries: 5,
        retry_base_delay: Duration::from_secs(2),
    };
    assert_eq!(config.max_connections, 5);
}

#[test]
fn retry_delay_uses_exponential_backoff() {
    let config = PoolConfig::default();
    // retry 0 = 1s, retry 1 = 2s, retry 2 = 4s
    assert_eq!(config.retry_delay(0), Duration::from_secs(1));
    assert_eq!(config.retry_delay(1), Duration::from_secs(2));
    assert_eq!(config.retry_delay(2), Duration::from_secs(4));
}
