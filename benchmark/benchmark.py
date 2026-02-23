import socket
import time
import threading
import sys

"""
StreamLine Concurrency & Latency Benchmarking
---------------------------------------------
This script simulates multiple concurrent TCP clients to measure:
1. RTT (round trip time) using the /ping command, simulating typical network ping protocol
2. Server throughput (operations per second)
3. Stability under high concurrency (1000+ connections)
"""

def benchmark_client(host, port, username, num_pings, results):
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(10)
        s.connect((host, port))
        
        f = s.makefile('rw')
        
        pings = []
        for i in range(num_pings):
            ts = int(time.time() * 1000)
            start = time.perf_counter()
            f.write(f"/ping {ts}\n")
            f.flush()
            
            while True:
                line = f.readline()
                if not line:
                    break
                if "/PONG" in line:
                    end = time.perf_counter()
                    pings.append((end - start) * 1000)
                    break
            if num_pings > 1:
                time.sleep(0.05)
            
        if pings:
            avg = sum(pings) / len(pings)
            results.append(avg)
            
        s.close()
    except Exception:
        pass

def run_stress_test(host, port, num_clients, pings_per_client):
    print(f"\n--- Testing {num_clients} Concurrent Clients ({pings_per_client} pings/client) ---")
    threads = []
    results = []
    
    start_time = time.perf_counter()
    
    for i in range(num_clients):
        t = threading.Thread(target=benchmark_client, args=(host, port, f"bot_{i}", pings_per_client, results))
        threads.append(t)
        t.start()
        
    for t in threads:
        t.join()
        
    end_time = time.perf_counter()
    duration = end_time - start_time
    
    if results:
        avg_rtt = sum(results) / len(results)
        print(f"Success Rate: {len(results)}/{num_clients}")
        print(f"Total Duration: {duration:.2f}s")
        print(f"Average Software RTT: {avg_rtt:.2f}ms")
        print(f"Throughput: {(len(results) * pings_per_client) / duration:.2f} ops/sec")
    else:
        print("No results collected, server may not be running")

if __name__ == "__main__":
    HOST = "127.0.0.1"
    PORT = 8000
    
    if len(sys.argv) > 1:
        PORT = int(sys.argv[1])
        
    print("StreamLine Performance Benchmark\n")
    
    # Baseline load - 1 TCP connection
    run_stress_test(HOST, PORT, 1, 10)
    
    # High load - 100 concurrent TCP connections
    run_stress_test(HOST, PORT, 100, 5)
    
    # Extreme load - 1000 concurrent TCP connections
    run_stress_test(HOST, PORT, 1000, 1)
