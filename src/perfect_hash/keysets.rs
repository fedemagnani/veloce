// Upstream's key sets, shared by the perfect_hash benches and probes so none can drift: no `//!`, as it's included

/// Calls `callback!` once with every set as `core|extra <name> { <key> => <index>, .. } misses [..]`, `extra` repeating a covered path
macro_rules! keysets {
    ($callback:ident) => {
        $callback! {
            // URL protocols: 6 keys, up to 5 bytes
            core protocol {
                "http" => 0, "https" => 1, "ftp" => 2, "ws" => 3, "wss" => 4, "file" => 5,
            }
            misses [
                "ssh", "telnet", "mailto", "data", "blob", "urn",
            ]
            // S&P 100 tickers: 100 keys, up to 5 bytes
            core stock {
                "AAPL" => 0, "ABBV" => 1, "ABT" => 2, "ACN" => 3, "ADBE" => 4, "AIG" => 5, "AMD" => 6,
                "AMGN" => 7, "AMT" => 8, "AMZN" => 9, "AVGO" => 10, "AXP" => 11, "BA" => 12, "BAC" => 13,
                "BK" => 14, "BKNG" => 15, "BLK" => 16, "BMY" => 17, "C" => 18, "CAT" => 19, "CHTR" => 20,
                "CL" => 21, "CMCSA" => 22, "COF" => 23, "COP" => 24, "COST" => 25, "CRM" => 26,
                "CSCO" => 27, "CVS" => 28, "CVX" => 29, "DE" => 30, "DHR" => 31, "DIS" => 32, "DOW" => 33,
                "DUK" => 34, "EMR" => 35, "EXC" => 36, "F" => 37, "FDX" => 38, "GD" => 39, "GE" => 40,
                "GILD" => 41, "GM" => 42, "GOOG" => 43, "GS" => 44, "HD" => 45, "HON" => 46, "IBM" => 47,
                "INTC" => 48, "INTU" => 49, "ISRG" => 50, "JNJ" => 51, "JPM" => 52, "KHC" => 53, "KO" => 54,
                "LIN" => 55, "LLY" => 56, "LMT" => 57, "LOW" => 58, "MA" => 59, "MCD" => 60, "MDLZ" => 61,
                "MDT" => 62, "MET" => 63, "META" => 64, "MMM" => 65, "MO" => 66, "MRK" => 67, "MS" => 68,
                "MSFT" => 69, "NEE" => 70, "NFLX" => 71, "NKE" => 72, "NVDA" => 73, "ORCL" => 74,
                "PEP" => 75, "PFE" => 76, "PG" => 77, "PM" => 78, "PYPL" => 79, "QCOM" => 80, "RTX" => 81,
                "SBUX" => 82, "SCHW" => 83, "SO" => 84, "SPG" => 85, "T" => 86, "TGT" => 87, "TMO" => 88,
                "TMUS" => 89, "TSLA" => 90, "TXN" => 91, "UNH" => 92, "UNP" => 93, "UPS" => 94, "USB" => 95,
                "V" => 96, "VZ" => 97, "WFC" => 98, "WMT" => 99,
            }
            misses [
                "RIVN", "PLTR", "SNAP", "UBER", "LYFT", "COIN", "HOOD", "DKNG", "SOFI", "RBLX", "ROKU",
                "ZM", "ABNB", "DASH", "CRWD", "NET", "SNOW", "DDOG", "MDB", "PATH",
            ]
            // C++ keywords: 15 keys, up to 6 bytes
            extra keyword {
                "auto" => 0, "bool" => 1, "break" => 2, "case" => 3, "char" => 4, "class" => 5,
                "const" => 6, "do" => 7, "double" => 8, "else" => 9, "enum" => 10, "float" => 11,
                "for" => 12, "goto" => 13, "if" => 14,
            }
            misses [
                "int", "void", "return", "while", "switch", "struct", "static", "extern", "inline",
                "virtual", "public", "private", "throw", "try", "catch",
            ]
            // HTTP headers: 20 keys, up to 15 bytes
            core header {
                "Accept" => 0, "Accept-Encoding" => 1, "Authorization" => 2, "Cache-Control" => 3,
                "Connection" => 4, "Content-Length" => 5, "Content-Type" => 6, "Cookie" => 7, "Date" => 8,
                "Host" => 9, "If-None-Match" => 10, "Location" => 11, "Origin" => 12, "Referer" => 13,
                "Server" => 14, "Set-Cookie" => 15, "User-Agent" => 16, "Vary" => 17, "Via" => 18,
                "X-Forwarded-For" => 19,
            }
            misses [
                "X-Request-Id", "Forwarded", "Alt-Svc", "DNT", "Upgrade-Insecure-Requests",
                "X-Frame-Options", "Content-Security-Policy", "Strict-Transport-Security", "Pragma",
                "Warning",
            ]
            // MIME types: 15 keys, up to 16 bytes
            extra mime {
                "text/html" => 0, "text/plain" => 1, "text/css" => 2, "application/json" => 3,
                "application/xml" => 4, "application/pdf" => 5, "application/zip" => 6, "image/png" => 7,
                "image/jpeg" => 8, "image/gif" => 9, "image/webp" => 10, "audio/mpeg" => 11,
                "video/mp4" => 12, "font/woff2" => 13, "font/woff" => 14,
            }
            misses [
                "text/xml", "text/csv", "application/gzip", "application/wasm", "image/avif", "image/tiff",
                "audio/ogg", "video/webm", "font/otf", "application/x-www-form-urlencoded",
            ]
            // Letters a-z: 26 single-byte keys
            core letters {
                "a" => 0, "b" => 1, "c" => 2, "d" => 3, "e" => 4, "f" => 5, "g" => 6, "h" => 7, "i" => 8,
                "j" => 9, "k" => 10, "l" => 11, "m" => 12, "n" => 13, "o" => 14, "p" => 15, "q" => 16,
                "r" => 17, "s" => 18, "t" => 19, "u" => 20, "v" => 21, "w" => 22, "x" => 23, "y" => 24,
                "z" => 25,
            }
            misses [
                "A", "B", "1", "!", "aa", "zz", "0", "ab", "yz", "mn", "?", "@", "+", "-", "_", ".", "/",
                "~", "*", "$",
            ]
            // HTTP headers: 50 keys, up to 19 bytes
            extra headers50 {
                "Accept" => 0, "Accept-Charset" => 1, "Accept-Encoding" => 2, "Accept-Language" => 3,
                "Accept-Ranges" => 4, "Age" => 5, "Allow" => 6, "Authorization" => 7, "Cache-Control" => 8,
                "Connection" => 9, "Content-Encoding" => 10, "Content-Language" => 11,
                "Content-Length" => 12, "Content-Location" => 13, "Content-Type" => 14, "Cookie" => 15,
                "Date" => 16, "ETag" => 17, "Expect" => 18, "Expires" => 19, "From" => 20, "Host" => 21,
                "If-Match" => 22, "If-Modified-Since" => 23, "If-None-Match" => 24, "If-Range" => 25,
                "If-Unmodified-Since" => 26, "Keep-Alive" => 27, "Last-Modified" => 28, "Location" => 29,
                "Max-Forwards" => 30, "Origin" => 31, "Pragma" => 32, "Proxy-Authenticate" => 33,
                "Proxy-Authorization" => 34, "Range" => 35, "Referer" => 36, "Retry-After" => 37,
                "Server" => 38, "Set-Cookie" => 39, "TE" => 40, "Trailer" => 41, "Transfer-Encoding" => 42,
                "Upgrade" => 43, "User-Agent" => 44, "Vary" => 45, "Via" => 46, "WWW-Authenticate" => 47,
                "Warning" => 48, "X-Forwarded-For" => 49,
            }
            misses [
                "X-Request-Id", "Forwarded", "Alt-Svc", "DNT", "Upgrade-Insecure-Requests",
                "X-Frame-Options", "Content-Security-Policy", "Strict-Transport-Security",
                "Access-Control-Allow-Origin", "Content-MD5",
            ]
            // JavaScript reserved words: 45 keys, up to 10 bytes
            extra jsreserved {
                "await" => 0, "break" => 1, "case" => 2, "catch" => 3, "class" => 4, "const" => 5,
                "continue" => 6, "debugger" => 7, "default" => 8, "delete" => 9, "do" => 10, "else" => 11,
                "enum" => 12, "export" => 13, "extends" => 14, "false" => 15, "finally" => 16, "for" => 17,
                "function" => 18, "if" => 19, "import" => 20, "in" => 21, "instanceof" => 22, "new" => 23,
                "null" => 24, "return" => 25, "super" => 26, "switch" => 27, "this" => 28, "throw" => 29,
                "true" => 30, "try" => 31, "typeof" => 32, "var" => 33, "void" => 34, "while" => 35,
                "with" => 36, "yield" => 37, "implements" => 38, "interface" => 39, "package" => 40,
                "private" => 41, "protected" => 42, "public" => 43, "static" => 44,
            }
            misses [
                "let", "console", "alert", "document", "window", "Array", "Object", "String", "Number",
                "Boolean", "undefined", "NaN", "Infinity", "Math", "Date", "RegExp", "JSON", "Promise",
                "async", "arguments",
            ]
            // Telemetry counters: 10 keys, up to 21 bytes
            core counters {
                "total_items" => 0, "successful_operations" => 1, "failed_operations" => 2, "warnings" => 3,
                "errors" => 4, "retries" => 5, "bytes_processed" => 6, "records_read" => 7,
                "records_matched" => 8, "records_written" => 9,
            }
            misses [
                "records_validated", "total_bytes", "processed_bytes", "failed_requests", "invalid_records",
                "total_records", "matched_records", "written_records", "ok_count", "error_count",
            ]
            // Java class names: 40 keys, 24 to 54 bytes
            core fqcn {
                "java.util.concurrent.ConcurrentHashMap" => 0,
                "java.util.concurrent.ConcurrentLinkedQueue" => 1,
                "java.util.concurrent.CopyOnWriteArrayList" => 2,
                "java.util.concurrent.atomic.AtomicInteger" => 3,
                "java.util.concurrent.atomic.AtomicLongArray" => 4,
                "java.util.concurrent.locks.ReentrantReadWriteLock" => 5,
                "java.util.concurrent.ScheduledThreadPoolExecutor" => 6,
                "java.util.concurrent.CompletableFuture" => 7,
                "java.util.concurrent.ThreadPoolExecutor" => 8,
                "java.util.concurrent.LinkedBlockingQueue" => 9, "java.util.function.BiFunction" => 10,
                "java.util.function.ToLongFunction" => 11, "java.util.stream.Collectors" => 12,
                "java.util.stream.IntStream" => 13, "java.util.regex.PatternSyntaxException" => 14,
                "java.lang.reflect.InvocationTargetException" => 15,
                "java.lang.reflect.ParameterizedType" => 16, "java.lang.invoke.MethodHandles" => 17,
                "java.lang.ref.WeakReference" => 18, "java.io.ByteArrayOutputStream" => 19,
                "java.io.ObjectInputStream" => 20, "java.nio.channels.AsynchronousSocketChannel" => 21,
                "java.nio.charset.StandardCharsets" => 22,
                "java.nio.file.attribute.BasicFileAttributes" => 23, "java.net.http.HttpClient" => 24,
                "java.security.MessageDigest" => 25, "java.security.spec.PKCS8EncodedKeySpec" => 26,
                "java.time.format.DateTimeFormatter" => 27, "java.time.temporal.ChronoUnit" => 28,
                "javax.crypto.spec.SecretKeySpec" => 29, "org.apache.commons.lang3.StringUtils" => 30,
                "org.apache.commons.collections4.MultiValuedMap" => 31,
                "org.springframework.context.ApplicationContext" => 32,
                "org.springframework.web.bind.annotation.RequestMapping" => 33,
                "org.springframework.beans.factory.annotation.Autowired" => 34,
                "com.fasterxml.jackson.databind.ObjectMapper" => 35,
                "com.fasterxml.jackson.annotation.JsonProperty" => 36,
                "com.google.common.collect.ImmutableList" => 37,
                "com.google.common.util.concurrent.ListenableFuture" => 38,
                "io.netty.channel.socket.nio.NioSocketChannel" => 39,
            }
            misses [
                "java.util.concurrent.ConcurrentSkipListMap", "java.util.concurrent.ForkJoinPool",
                "java.util.concurrent.atomic.AtomicReference", "java.util.function.Supplier",
                "java.lang.reflect.Method", "java.io.FileInputStream", "java.nio.ByteBuffer",
                "java.time.ZonedDateTime", "org.apache.commons.io.FileUtils",
                "org.springframework.stereotype.Component", "com.fasterxml.jackson.core.JsonParser",
                "com.google.common.base.Preconditions", "io.netty.buffer.ByteBuf",
                "javax.servlet.http.HttpServletRequest", "java.util.concurrent.ConcurrentHashMap$Node",
                "java.util.concurrent.ConcurrentHashMa",
                "org.springframework.web.bind.annotation.RequestMappings",
                "com.google.common.collect.ImmutableLists", "java.util.stream.Collector",
                "java.security.MessageDigests",
            ]
        }
    };
}
