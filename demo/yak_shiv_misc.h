
// Yak Shiv misc library - when merely shaving the yak isn't a permanent enough solution.
// 
// A tiny library of handy routines by Tom Forsyth (eelpi.gotdns.org)

#include <intrin.h> // for __debugbreak()

#if defined(_DEBUG) || defined(DEBUG)
#undef DEBUG
#define DEBUG
#undef _DEBUG
#define _DEBUG
#endif

typedef signed char sint8;
typedef unsigned char uint8;
typedef signed short sint16;
typedef unsigned short uint16;
typedef signed long sint32;
typedef unsigned long uint32;
typedef signed __int64 sint64;
typedef unsigned __int64 uint64;
typedef unsigned long bool32;


#ifdef DEBUG

#define ASSERT(exp) { if (!(exp)){__debugbreak();} }
#define ASSERTONCE(exp) { static bool done = false; if (!done && !(exp)){ done = true; __debugbreak();} }
#define DEFAULT_ASSERT() default: ASSERT ( false ); break

#else //#ifdef DEBUG

#define ASSERT sizeof
#define ASSERTONCE sizeof
#define DEFAULT_ASSERT() default: break

#endif //#else //#ifdef DEBUG

#define MIN(a,b) ( (a) > (b) ? (b) : (a) )
#define MAX(a,b) ( (a) < (b) ? (b) : (a) )

template <class T> T Min ( T a, T b )
{
    if ( a > b )
    {
        return b;
    }
    return a;
}

template <class T> T Max ( T a, T b )
{
    if ( a > b )
    {
        return a;
    }
    return b;
}

template <class T> T Clamp ( T a, T min, T max )
{
    ASSERT ( min <= max );
    if ( a > min )
    {
        if ( a < max )
        {
            return a;
        }
        return max;
    }
    return min;
}

#define AtLeast(x,y) Max(x,y)
#define AtMost(x,y) Min(x,y)

template <class S> void Swap ( S &a, S &b )
{
	S temp = a;
	a = b;
	b = temp;
}

// 0...1 in, 0...1 out.
inline float SmoothStep ( float in )
{
    return in*in*(3.0f-2.0f*in);
}

// Originally from https://code.google.com/p/chromium/codesearch#chromium/src/third_party/webrtc/base/arraysize.h&q=arraysize&sq=package:chromium&l=1
template <typename T, size_t N> char (&ArraySizeHelper(T (&array)[N]))[N];
#define arraysize(array) (sizeof(ArraySizeHelper(array)))
#undef ARRAYSIZE
#define ARRAYSIZE(x) (arraysize(x))
#define ArraySize(x) (arraysize(x))
#define NumItemsIn(x)(arraysize(x))


#define ASSERT_CLOSE_ENOUGH(x,y,tolerance) ASSERT(fabsf((x)-(y)) < (tolerance))


static float const PI = 3.14159274101257324f; // Not sure why Wikipedia gives so many decimal places for a float32

// Time class.
//
// Using floating point for wall-clock time is a terrible idea!
// 
// float32 is a disaster - it has only 24 bits of precision, so after 5 hours, milliseconds don't exist any more!
// Use integers instead - consistent reliable precision, and there's nothing special about "time 0" (whatever that is)
// 
// This class uses the same format at NTP: https://en.wikipedia.org/wiki/Network_Time_Protocol#Timestamps
// 32 bits for seconds, 32 bits for fractions of a second.
// This has a consistent precision of 233 picoseconds and wraps every 136 years, which should be enough for most games.
// 
// For more, see https://tomforsyth1000.github.io/blog.wiki.html#%5B%5BA%20matter%20of%20precision%5D%5D 
//
// IMPORTANT - note that there is no way to ask it what the "current" time is. That's almost always not a useful concept,
// just as it is not useful to ask how many seconds it has been since the Battle Of Hastings.
// You should ask "how many seconds since this other NtpTimeStamp" - and it's totally fine to return that as a float,
// since the value 0.0f has a sensible meaning.

class NtpTimeStamp
{
private:
    float const FromSeconds = (float)(1llu<<32);
    float const ToSeconds = 1.0f / FromSeconds;

public:
    NtpTimeStamp() : counter(0) {}

    void Init()
    {
        counter = 0;
    }

    void operator= (NtpTimeStamp other)
    {
        counter = other.counter;
    }

    bool operator== (NtpTimeStamp other) { return ( counter == other.counter ); }
    bool operator>  (NtpTimeStamp other) { return ( counter >  other.counter ); }
    bool operator>= (NtpTimeStamp other) { return ( counter >= other.counter ); }
    bool operator<  (NtpTimeStamp other) { return ( counter <  other.counter ); }
    bool operator<= (NtpTimeStamp other) { return ( counter <= other.counter ); }
    bool operator!= (NtpTimeStamp other) { return ( counter != other.counter ); }

    float [[nodiscard]] SecondsSince ( NtpTimeStamp EarlierTimeStamp ) const
    {
        sint64 delta = counter - EarlierTimeStamp.counter;
        return (float)delta * ToSeconds;
    }

    NtpTimeStamp SecondsAfter ( float Seconds ) const
    {
        sint64 delta = (sint64)floorf ( Seconds * FromSeconds + 0.5f );
        NtpTimeStamp result;
        result.counter = counter + delta;
        return result;
    }

    void AddSeconds ( float Seconds )
    {
        sint64 delta = (sint64)floorf ( Seconds * FromSeconds + 0.5f );
        counter += delta;
    }

private:

    sint64 counter;
};

// A version of NtpTimeStamp that will use GetPerformanceCounter to keep up to date with "wall clock" time.
class NtpTimeStampGetPerformanceCounter
{
public:
    NtpTimeStampGetPerformanceCounter() : qpcCountsPerSecondReciprocal (0.0f) {}

    void Init()
    {
        ::QueryPerformanceFrequency(&qpcCountsPerSecond);
        ::QueryPerformanceCounter(&qpcCurrent);
        qpcCountsPerSecondReciprocal = 1.0f / (float)qpcCountsPerSecond.QuadPart;
    }

    // Returns the number of seconds since the last update.
    float UpdateRealTime()
    {
        qpcLast = qpcCurrent;
        ::QueryPerformanceCounter(&qpcCurrent);

        sint64 Delta = qpcCurrent.QuadPart - qpcLast.QuadPart;
        float SecondsPassed = qpcCountsPerSecondReciprocal * (float)Delta;
        CurrentTimeStamp.AddSeconds ( SecondsPassed );
        return SecondsPassed;
    }

    NtpTimeStamp Current() const
    {
        return CurrentTimeStamp;
    }

private:
    LARGE_INTEGER qpcCountsPerSecond = {};
    float qpcCountsPerSecondReciprocal = 0.0f;
    LARGE_INTEGER qpcCurrent = {};
    LARGE_INTEGER qpcLast = {};
    NtpTimeStamp CurrentTimeStamp = {};
};




