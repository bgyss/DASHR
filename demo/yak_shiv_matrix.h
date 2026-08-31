
// Yak Shiv matrix library - when merely shaving the yak isn't a permanent enough solution.
// 
// A highly opinionated vector & matrix library by Tom Forsyth (eelpi.gotdns.org)
// 
// *** TO USE
// 
// For one .c/.cpp file in your project, do:
// 
// Do this:
//      #define YAK_SHIV_MATRIX_IMPLEMENTATION
// before you include this file in *one* C or C++ file to create the implementation.
// 
// *** FEATURES
// 
// - Row-major, column vectors.
// 
// - No SIMD. Too complex, no benefit - see "Background"
// 
// - Moderately strongly typed, moderately type-safe, and uses C++. Does use overloading, but in a mostly-safe way.
// 
// - Generic Vec4 and Mat44 types. Not very type-safe or efficient. Try not to fall back to them.
//
// *** BRIEF DESCRIPTION
//
// Most safe types are three letters. Most unsafe types are 4 or more. All values are float32 unless stated. The following exist:
// 
// Name         Long name           Elements                Notes
//  Vec2: 	                        (x, y)                  Not very type-safe.
//  Vec3: 	                        (x, y, z)               Not very type-safe.
//  Vec4: 	                        (x, y, z, w)            Not very type-safe.
//  Quat                            (x, y, z, w)            General quaternion. Not very type-safe.
//  Pos:  	    Position            (x, y, z, 1)            Final "1" is not stored.
//  Dir:  	    Direction           (x, y, z, 0)            Final "0" is not stored. Used for normals or offsets or tangents
//  Rot:  	    Rotation            3x3 matrix              Assumed orthonormal, scales/shears will do odd things.
//  Qot:  	    Quaternion rot      (x, y, z, w)            Assumed normalized, if not will do odd things.
//  Orn:	    Orientation         3x4 matrix              Rot + Pos. Assumed orthonormal, scales/shears will do odd things.
//  Qrn:	    Quaternion + pos    Qot + Pos               Combined 7-element thingie. "Qrn" = "Quaternion Orientation"
//  Mat44:	                        4x4 matrix              Not very type-safe.
//  WorldPos:                       int64 (x, y, z)         Fixed point position for both huge and tiny worlds.
//  WorldOrn:   Orn with WorldPos   3x3 + WorldPos          Fundamental descriptor of an objects' position and orientation in world space.
//  WorldQrn:   Qrn with WorldPos   Quat + WorldPos         Same, but using a quaternion.
//
// Important:
// 
// - There is operator overloading to help you do the right thing. But it will also try to stop you doing the wrong thing.
// 
// - Most operations are somewhat typesafe to prevent you doing things that are probably errors. See "Details" for info.
//
// - There are no autoconversions. If you want to convert use the .ToXXX() or .GetXXX() methods.
// 
// - You can work around the type system by converting everything to Vec4 and Mat44, but you shouldn't.
// 
// - If you ever give these matrices to D3D/HLSL, you will want to start your HLSL with:
//      #pragma pack_matrix(row_major)
//      ...or qualify each matrix separately with "row_major"
//      ...or use the /Zpr directive.
//      Note DANGER WILL ROBINSON there is a complex interaction with StructuredBuffers and the /Zpr directive
//      See: https://mastodon.gamedev.place/@Maraneshi/116622745463959988
// 
// *** DETAILS
// 
// This library leans on operator overloading and type safety in a very specific way. It lets you do
// common sensible inline operators very easily, but deliberately does not implement inline operators that are
// ambigouous or risky. To do those, use an explicit method, or convert one operand to a different one explicitly.
// If you're really really sure you know what you're doing, convert things to the generic Vec4 and Mat44.
// 
// For the same reason, there are no automatic type conversions supported. All conversions are explicit
// (but possibly cheap) to make sure you get as few surprises as possible.
// 
// In general, methods use the following naming conventions:
// 
// .AsXXX():    Bitcast to XXX. Implies no work. Typically returns an aliasing const reference - careful now!
// .ToXXX(): 	Simple conversion, e.g. Pos.ToVec4() will add the final 1.0. The compiler may remove it again.
// .GetXXX(): 	More complex conversion, e.g. Qot.GetRot() to turn a quaternion into a mat3x3
//              Can also extract a subitem from an object e.g. pos = orn.GetPos().
// 			    This can also perform more complex operations on the thing and return a new value, e.g. Dir.GetNormalized()
// .MakeXXX():  Perform operation in-place. Does not return a value, e.g. Dir.MakeNormalized()
// .SetXXX():	Set a component of the object, e.g. orn.SetPos(newPos).
// 
// *** BACKGROUND
// 
// This started when I "just" needed a simple maths library to do a cheesy demo.
// I found a few existing ones, but they were somewhat annoying to use,
// and many of them don't use the same vectors-are-columns and row-major
// defaults that I prefer.
//
// After a bunch of rabbit-holing and internet discussion I decided to actually
// write down what I liked and why, so I wouldn't have to relearn this all again,
// like I do every five years before forgetting and getting confused.
// 
// Some background on terminology:
// https://fgiesen.wordpress.com/2012/02/12/row-major-vs-column-major-row-vectors-vs-column-vectors/
//
// First, I conducted a poll of my fellow nerds:
// https://mastodon.gamedev.place/@TomF/116565769021563620
// 
// 28%: Vectors-are-rows, row-major storage
// 39%: Vectors-are-columns, row-major storage
//  6%: Vectors-are-rows, column-major storage
// 27%: Vectors-are-columns, column-major storage
// (104 people voted)
//
// Pretty clear winner, and basically tied for second place.
// It should be noted that the two second places correspond to
// D3D and OpenGL defaults, so a tie is amusing, but also even with
// the advantage of being the incumbents, my fellow nerds still say "no" to them.
// (note that HLSL/GLSL can support all four options, so it's not a decider)
// 
// This actually makes life easy. I was debating whether to support a second
// format, even though it's annoying. But I'm not going to support three,
// so therefore I shall only support one, and it's the one I prefer.
// Everything is working out fine.
// 
// Just to further confirm my biases, a table of pros and cons:
// 
//                                                          ---Row major storage---     --Column major storage--
//                                   Reason     Points      Row vect    Column vect     Row vect    Column vect
//                                   ---------------------------------------------------------------------------
//           Answer on left side, same as =        1     |                  y                           y
//     Memory order = English reading order        2     |      y           y
// Access right/up/fwd/pos vectors directly        1     |      y                                       y
//                      Direct HLSL support        2     |      y           y               y           y
//  (0,0,0,1) is last in memory, can dscard        2     |                  y               y
//                             --------------------------+-----------------------------------------------------
//                             Points total              |      5           7               4           4
//
// There you go - it's simply science(TM). Vectors-are-columns, row-major storage is the best.
// 
// That decision made, it has the following consequences:
// 
// - M*M is *always* rows of first times columns of second. This is true in all four schemes.
// 
// - M*V is how you do matrix*vector. Can't be the other way round.
// 
// - M2*M1*V means "take V, apply M1, then apply M2". Though it does associate, i.e. M2*(M1*V) == (M2*M1)*V
// 
// - Matrices should use the "XfromY" naming scheme, which means that when you multiply them together
//      the names help you get it right, e.g.
// 
//      cameraFromTurret = cameraFromWorld * worldFromTank * tankFromTurret;
//         P        S         P        Q--------Q      R-------R        S
// 
//      You can't multiply them in any other order and still have the words match as shown.
//      Also, you'll typically have "worldFromCamera", so you know you need to first take the inverse to find "cameraFromWorld"
// 
//      See https://tomforsyth1000.github.io/blog.wiki.html#%5B%5BMatrix%20maths%20and%20names%5D%5D
// 
// - An affine transform (rotation & translation) 4x4 matrix can be thought of as four columns:
//      1st column = X basis vector
//      2nd column = Y basis vector
//      3rd column = Z basis vector
//      4th column = translation vector
//      Last row is always (0,0,0,1), and so a lot of the time can just be assumed, not stored.
// 
// - HLSL is sneaky and supports all modes. If you write "mul(M,V)" it uses column vectors. If you write "mul(V,M)" it uses row vectors.
//   You can use to "row_matrix" or "column_matrix" directives to specify which, on a per-matrix basis, AND YOU SHOULD. Defaults are bad.
// 
// - Using this library in OpenGL (or gods help you OpenGLES) will take some care. Sorry not sorry, this is my yak, not yours.
// 
//
// To explain the "no SIMD support" part, there's a few parts to it:
// 
// - This sort of vector maths typically has very little "elegant" SIMD, in the way we think of GPUs as being elegant SIMD.
//   There is a lot of shuffling of components and things rarely line up for a nice vec4 multiply-add.
//   You can look at the DirectXMath.h library for a good-as-it-gets SSE implementation, but it's inherently got a lot of
//   shuffles and scalar maths. The wins here are small and annoying.
// 
// - Whatever minor SIMD does naturally exist, most compilers will find it for you.
// 
// - Most of this sort of maths is limited by cache and memory performance, not instruction throughput.
// 
// - SIMD & intrinsics are a lot of work to support through compiler versions and architectures.
// 
// - If you are doing "bulk processing" where instruction throughput is important, you should
//   listen to folks like Mike Acton, AOSOA-ise all your data and write some lovely AVX512 intrinsics.
//   This is, of course, way out of scope for this library!
// 
//
// Random FAQs:
// 
// Q: why don't matrices use e.g. m[3][3] instead of Vec3 v[3], when the vectors don't "mean" anything,
//    i.e. they are not the x/y/z basic vectors?
// 
// A: because of type-safety. It is annoyingly easy to accidentally ask an Orn for m[1][3]. This
//    is an illegal value, but because it's still actually a real value (it is m[2][0]) it won't be
//    caught by most validators. However, Orn.y.w is simply a compile error - easily found.
//  
// Q: is this libray left or right handed?
//
// A: no. It's not inherently handed in any way except for the projection matrix calls which
//    take a bool to let you decide. I certainly have my preference, but it's simple enough
//    to shove it all in the projection matrix.
// 
// Q: Isn't "orientation" slightly misleading, since it implies an absolute value, but in practice
//    it is always the orientation of one thing relative to another?
// 
// A: this is correct. I just couldn't think of a neater name.
// 
// Q: should there be a version of "Dir" that implies unit-length, just like "Qot" is a unit-length "Quat"?
// 
// A: I can see the argument for it. However I'm not sure if it has much practical impact.
//    Assuming a "Rot" is orthonormal means you can skip a bunch of costly operations when taking the
//    inverse, which is very common, because that is the common case. Storing a non-orthonormal matrix
//    in a 3x3 is an uncommon thing to do, so saying "hey just use a Mat44" seems reasonable.
//    However, it is very common to use non-unit-length vectors from one thing to another,
//    so assuming you can skip the Normalise() call is not safe.
// 
// Q: what's with "normalise"?
// 
// A: this is my yak, my yak is highly opinionated, and uses English spelling.
//

class Dir;
class Pos;
class Rot;
class Qot;
class Orn;
class Qrn;
class Vec2;
class Vec3;
class Vec4;
class Mat44;
class Quat;

class Vec2
{
public:
    float x, y;

    inline Vec2() : x(0.0f), y(0.0f) {}
    inline Vec2 ( float x, float y ) : x(x), y(y) {}
    inline static Vec2 FromFloatPtr ( float const* ptr ) { return Vec2 ( ptr[0], ptr[1] ); }

    inline Vec2 operator+ ( Vec2 other ) const { return Vec2 ( x + other.x, y + other.y ); }
    inline Vec2 operator- ( Vec2 other ) const { return Vec2 ( x - other.x, y - other.y ); }
    inline Vec2 operator- () const { return Vec2 ( -x, -y ); }
    inline void operator+= ( Vec2 other ) { x += other.x; y += other.y; }
    inline Vec2 operator* ( float scale ) const { return Vec2 ( x * scale, y * scale ); }
    inline void operator*= ( float scale ) { x *= scale; y *= scale; }
    inline float Dot ( Vec2 other ) const { return ( x * other.x ) + ( y * other.y ); }
    inline float GetLengthSq() const { return ( x * x + y * y ); }
    inline float GetLength() const { return sqrtf ( x * x + y * y ); }
    inline Vec2 GetNormalise() const
    {
        float invLength = 1.0f / sqrtf ( x * x + y * y );
        return Vec2 ( x * invLength, y * invLength );
    }

    // Constants.
    static Vec2 zero;

    // Conversions.
    inline float* AsFloatPtr() { return &x; }
};

class Vec3
{
public:
    float x, y, z;

    inline Vec3() : x(0.0f), y(0.0f), z(0.0f) {}
    inline Vec3 ( float x, float y, float z ) : x(x), y(y), z(z) {}
    inline static Vec3 FromFloatPtr ( float const* ptr ) { return Vec3 ( ptr[0], ptr[1], ptr[2] ); }

    inline Vec3 operator+ ( Vec3 other ) const { return Vec3 ( x + other.x, y + other.y, z + other.z ); }
    inline Vec3 operator- ( Vec3 other ) const { return Vec3 ( x - other.x, y - other.y, z - other.z ); }
    inline Vec3 operator- () const { return Vec3 ( -x, -y, -z ); }
    inline void operator+= ( Vec3 other ) { x += other.x; y += other.y; z += other.z; }
    inline Vec3 operator* ( float scale ) const { return Vec3 ( x * scale, y * scale, z * scale ); }
    inline void operator*= ( float scale ) { x *= scale; y *= scale; z *= scale; }
    inline float Dot ( Vec3 other ) const { return ( x * other.x ) + ( y * other.y ) + ( z * other.z ); }
    inline Vec3 Cross ( Vec3 other ) const { return Vec3 ( y * other.z - z * other.y, z * other.x - x * other.z, x * other.y - y * other.x ); }
    inline float GetLengthSq() const { return ( x * x + y * y + z * z ); }
    inline float GetLength() const { return sqrtf ( x * x + y * y + z * z ); }
    inline Vec3 GetNormalise() const
    {
        float invLength = 1.0f / sqrtf ( x * x + y * y + z * z );
        return Vec3 ( x * invLength, y * invLength, z * invLength );
    }

    // Constants.
    static Vec3 zero;

    // Conversions.
    inline float* AsFloatPtr() { return &x; }
    Dir ToDir() const; // defined later.
    Pos ToPos() const; // defined later.
    Vec4 ToVec4 ( float w ) const; // defined later.
};

class Vec4
{
public:
    float x, y, z, w;

    inline Vec4() : x(0.0f), y(0.0f), z(0.0f), w(0.0f) {}
    inline Vec4 ( float x, float y, float z, float w ) : x(x), y(y), z(z), w(w) {}
    inline static Vec4 FromFloatPtr ( float const* ptr ) { return Vec4 ( ptr[0], ptr[1], ptr[2], ptr[3] ); }

    inline Vec4 operator+ ( Vec4 other ) const { return Vec4 ( x + other.x, y + other.y, z + other.z, w + other.w ); }
    inline Vec4 operator- ( Vec4 other ) const { return Vec4 ( x - other.x, y - other.y, z - other.z, w - other.w ); }
    inline Vec4 operator- () const { return Vec4 ( -x, -y, -z, -w ); }
    inline void operator+= ( Vec4 other ) { x += other.x; y += other.y; z += other.z; w += other.w; }
    inline Vec4 operator* ( float scale ) const { return Vec4 ( x * scale, y * scale, z * scale, w * scale ); }
    inline void operator*= ( float scale ) { x *= scale; y *= scale; z *= scale; w *= scale; }
    inline float Dot ( Vec4 other ) const { return ( x * other.x ) + ( y * other.y ) + ( z * other.z ) * ( w * other.w ); }
    inline float GetLengthSq() const { return ( x * x + y * y + z * z + w * w ); }
    inline float GetLength() const { return sqrtf ( x * x + y * y + z * z + w * w ); }
    inline Vec4 GetNormalise() const
    {
        float invLength = 1.0f / sqrtf ( x * x + y * y + z * z + w * w );
        return Vec4 ( x * invLength, y * invLength, z * invLength, w * invLength );
    }

    // Constants.
    static Vec4 zero;

    // Conversions.
    inline float* AsFloatPtr() { return &x; }
    Dir ToDir() const; // defined later.
    Pos ToPos() const; // defined later.
    Quat ToQuat() const; // defined later.
    Vec3 ToVec3() const; // defined later.
};

class Mat44
{
public:
    // WARNING - these are ROW vectors, and do not "mean" very much.
    // Specifically, they are NOT the x/y/z/translation basis vectors. Use GetBasisX/Y/Z to get those.
    Vec4 x, y, z, w;

    inline Mat44() {}
    inline Mat44 ( Vec4 x, Vec4 y, Vec4 z, Vec4 w ) : x(x), y(y), z(z), w(w) {}
    inline static Mat44 FromFloatPtr ( float const* ptr ) { return Mat44 ( Vec4::FromFloatPtr ( ptr + 0 ),
                                                                           Vec4::FromFloatPtr ( ptr + 4 ),
                                                                           Vec4::FromFloatPtr ( ptr + 8 ),
                                                                           Vec4::FromFloatPtr ( ptr + 12 ) ); }

    inline Mat44 operator* ( Mat44 other ) const
    {
        Mat44 result;
        result.x.x = (x.x * other.x.x) + (x.y * other.y.x) + (x.z * other.z.x) + (x.w * other.w.x);
        result.x.y = (x.x * other.x.y) + (x.y * other.y.y) + (x.z * other.z.y) + (x.w * other.w.y);
        result.x.z = (x.x * other.x.z) + (x.y * other.y.z) + (x.z * other.z.z) + (x.w * other.w.z);
        result.x.w = (x.x * other.x.w) + (x.y * other.y.w) + (x.z * other.z.w) + (x.w * other.w.w);
        result.y.x = (y.x * other.x.x) + (y.y * other.y.x) + (y.z * other.z.x) + (y.w * other.w.x);
        result.y.y = (y.x * other.x.y) + (y.y * other.y.y) + (y.z * other.z.y) + (y.w * other.w.y);
        result.y.z = (y.x * other.x.z) + (y.y * other.y.z) + (y.z * other.z.z) + (y.w * other.w.z);
        result.y.w = (y.x * other.x.w) + (y.y * other.y.w) + (y.z * other.z.w) + (y.w * other.w.w);
        result.z.x = (z.x * other.x.x) + (z.y * other.y.x) + (z.z * other.z.x) + (z.w * other.w.x);
        result.z.y = (z.x * other.x.y) + (z.y * other.y.y) + (z.z * other.z.y) + (z.w * other.w.y);
        result.z.z = (z.x * other.x.z) + (z.y * other.y.z) + (z.z * other.z.z) + (z.w * other.w.z);
        result.z.w = (z.x * other.x.w) + (z.y * other.y.w) + (z.z * other.z.w) + (z.w * other.w.w);
        result.w.x = (w.x * other.x.x) + (w.y * other.y.x) + (w.z * other.z.x) + (w.w * other.w.x);
        result.w.y = (w.x * other.x.y) + (w.y * other.y.y) + (w.z * other.z.y) + (w.w * other.w.y);
        result.w.z = (w.x * other.x.z) + (w.y * other.y.z) + (w.z * other.z.z) + (w.w * other.w.z);
        result.w.w = (w.x * other.x.w) + (w.y * other.y.w) + (w.z * other.z.w) + (w.w * other.w.w);
        return result;
    }

    inline Vec4 operator* ( Vec4 other ) const
    {
        Vec4 result;
        result.x = (x.x * other.x) + (x.y * other.y) + (x.z * other.z) + (x.w * other.w);
        result.y = (y.x * other.x) + (y.y * other.y) + (y.z * other.z) + (y.w * other.w);
        result.z = (z.x * other.x) + (z.y * other.y) + (z.z * other.z) + (z.w * other.w);
        result.w = (w.x * other.x) + (w.y * other.y) + (w.z * other.z) + (w.w * other.w);
        return result;
    }

    inline Mat44 GetTranspose() const
    {
        return Mat44 ( Vec4 ( x.x, y.x, z.x, w.x ),
                       Vec4 ( x.y, y.y, z.y, w.y ),
                       Vec4 ( x.z, y.z, z.z, w.z ),
                       Vec4 ( x.w, y.w, z.w, w.w ) );
    }

    Mat44 GetInverse() const;  // defined later.

    // Constants.
    static Mat44 identity;

    // Conversions:
    inline float* AsFloatPtr() { return &x.x; }
    Rot ToRot() const; // defined later.
    Rot GetRot() const; // defined later.
    inline Vec4 GetBasisX() const { return Vec4 ( x.x, y.x, z.x, w.x ); }
    inline Vec4 GetBasisY() const { return Vec4 ( x.y, y.y, z.y, w.y ); }
    inline Vec4 GetBasisZ() const { return Vec4 ( x.z, y.z, z.z, w.z ); }
    inline Vec4 GetBasisW() const { return Vec4 ( x.w, y.w, z.w, w.w ); }
    inline void SetBasisX ( Vec4 in ) { x.x = in.x; y.x = in.y; z.x = in.z; w.x = in.w; }
    inline void SetBasisY ( Vec4 in ) { x.y = in.x; y.y = in.y; z.y = in.z; w.y = in.w; }
    inline void SetBasisZ ( Vec4 in ) { x.z = in.x; y.z = in.y; z.z = in.z; w.z = in.w; }
    inline void SetBasisW ( Vec4 in ) { x.w = in.x; y.w = in.y; z.w = in.z; w.w = in.w; }
    Orn ToOrn() const; // defined later.
};

class Quat // general quaternion, can be non-unit.
{
public:
    float x, y, z, w;

    inline Quat() : x(0.0f), y(0.0f), z(0.0f), w(1.0f) {}
    inline Quat ( float x, float y, float z, float w ) : x(x), y(y), z(z), w(w) {}
    inline static Quat FromFloatPtr ( float const* ptr ) { return Quat ( ptr[0], ptr[1], ptr[2], ptr[3] ); }

    // Constants.
    static Quat identity;

    // Conversions:
    inline float* AsFloatPtr() { return &x; }
    inline Vec4 ToVec4() const { return Vec4 ( x, y, z, w ); }
};

class Dir // Direction
{
public:
    // (x, y, z, 0.0). The final 0.0 is not stored.
    float x, y, z;

    inline Dir() : x(0.0f), y(0.0f), z(0.0f) {}
    inline Dir ( float x, float y, float z ) : x(x), y(y), z(z) {}
    inline static Dir FromFloatPtr ( float const* ptr ) { return Dir ( ptr[0], ptr[1], ptr[2] ); }

    inline Dir operator+ ( Dir other ) const { return Dir ( x + other.x, y + other.y, z + other.z ); }
    inline Dir operator- ( Dir other ) const { return Dir ( x - other.x, y - other.y, z - other.z ); }
    inline Dir operator- () const { return Dir ( -x, -y, -z ); }
    inline void operator+= ( Dir other ) { x += other.x; y += other.y; z += other.z; }
    inline Dir operator* ( float scale ) const { return Dir ( x * scale, y * scale, z * scale ); }
    inline void operator*= ( float scale ) { x *= scale; y *= scale; z *= scale; }
    inline float Dot ( Dir other ) const { return ( x * other.x ) + ( y * other.y ) + ( z * other.z ); }
    inline Dir Cross ( Dir other ) const { return Dir ( y * other.z - z * other.y, z * other.x - x * other.z, x * other.y - y * other.x ); }
    inline float GetLengthSq() const { return ( x * x + y * y + z * z ); }
    inline float GetLength() const { return sqrtf ( x * x + y * y + z * z ); }
    inline Dir GetNormalise() const
    {
        float invLength = 1.0f / sqrtf ( x * x + y * y + z * z );
        return Dir ( x * invLength, y * invLength, z * invLength );
    }

    // Constants.
    static Dir zero;

    // Conversions.
    inline float* AsFloatPtr() { return &x; }
    inline Vec3 ToVec3() const { return Vec3 ( x, y, z ); } // TODO: convert to AsVec3?
    inline Vec4 ToVec4() const { return Vec4 ( x, y, z, 0.0f ); }
};

class Pos // Position
{
public:
    // (x, y, z, 1.0). The final 1.0 is not stored.
    float x, y, z;
    
    inline Pos() : x(0.0f), y(0.0f), z(0.0f) {}
    inline Pos ( float x, float y, float z ) : x(x), y(y), z(z) {}
    inline static Pos FromFloatPtr ( float const* ptr ) { return Pos ( ptr[0], ptr[1], ptr[2] ); }

    // Most operations between two Pos (addiiton, Dot, etc) are probably not wise.
    // The only sensible thing is subtraction of two Pos to produce a Dir.
    inline Dir operator- ( Pos other ) const { return Dir ( x - other.x, y - other.y, z - other.z ); }
    // Maths operations only with a Dir, not with another Pos.
    inline Pos operator+ ( Dir other ) const { return Pos ( x + other.x, y + other.y, z + other.z ); }
    inline Pos operator- ( Dir other ) const { return Pos ( x - other.x, y - other.y, z - other.z ); }
    inline void operator+= ( Dir other ) { x += other.x; y += other.y; z += other.z; }

    // Deliberately no identity/zero for Positions.

    // Conversions.
    inline float* AsFloatPtr() { return &x; }
    inline Vec3 ToVec3() const { return Vec3 ( x, y, z ); } // TODO: convert to AsVec3?
    inline Vec4 ToVec4() const { return Vec4 ( x, y, z, 1.0f ); }
};

class Rot // Rotation as a matrix
{
public:
    // WARNING - these are ROW vectors, and do not "mean" very much.
    // Specifically, they are NOT the x/y/z basis vectors. Use GetBasisX/Y/Z to get those.
    // WARNING this mat3x3 is assumed orthonormalized. If it is not, odd things will happen.
    Vec3 x, y, z;

    inline Rot() {}
    inline Rot ( Vec3 x, Vec3 y, Vec3 z ) : x(x), y(y), z(z) {}
    inline Rot ( Dir x, Dir y, Dir z ) : x(x.ToVec3()), y(y.ToVec3()), z(z.ToVec3()) {}
    inline void operator= ( Rot other ) { x = other.x; y = other.y; z = other.z; }
    inline static Rot FromFloatPtr ( float const* ptr ) { return Rot ( Vec3::FromFloatPtr ( ptr + 0 ),
                                                                       Vec3::FromFloatPtr ( ptr + 3 ),
                                                                       Vec3::FromFloatPtr ( ptr + 6 ) ); }
    inline static Rot MakeFromBasis ( Dir basisX, Dir basisY, Dir basisZ )
    {
        return Rot ( Vec3 ( basisX.x, basisY.x, basisZ.x ),
                     Vec3 ( basisX.y, basisY.y, basisZ.y ),
                     Vec3 ( basisX.z, basisY.z, basisZ.z ) );
    }
    static Rot MakeRotateX ( float angleInRadians );
    static Rot MakeRotateY ( float angleInRadians );
    static Rot MakeRotateZ ( float angleInRadians );

    inline Rot operator* ( Rot other ) const
    {
        Rot result;
        result.x.x = (x.x * other.x.x) + (x.y * other.y.x) + (x.z * other.z.x);
        result.x.y = (x.x * other.x.y) + (x.y * other.y.y) + (x.z * other.z.y);
        result.x.z = (x.x * other.x.z) + (x.y * other.y.z) + (x.z * other.z.z);
        result.y.x = (y.x * other.x.x) + (y.y * other.y.x) + (y.z * other.z.x);
        result.y.y = (y.x * other.x.y) + (y.y * other.y.y) + (y.z * other.z.y);
        result.y.z = (y.x * other.x.z) + (y.y * other.y.z) + (y.z * other.z.z);
        result.z.x = (z.x * other.x.x) + (z.y * other.y.x) + (z.z * other.z.x);
        result.z.y = (z.x * other.x.y) + (z.y * other.y.y) + (z.z * other.z.y);
        result.z.z = (z.x * other.x.z) + (z.y * other.y.z) + (z.z * other.z.z);
        return result;
    }

    inline void operator*= ( Rot other ) { *this = *this * other; }

    inline Dir operator* ( Dir other ) const
    {
        Dir result;
        result.x = (x.x * other.x) + (x.y * other.y) + (x.z * other.z);
        result.y = (y.x * other.x) + (y.y * other.y) + (y.z * other.z);
        result.z = (z.x * other.x) + (z.y * other.y) + (z.z * other.z);
        return result;
    }

    inline Rot GetTranspose() const
    {
        return Rot ( Vec3 ( x.x, y.x, z.x ),
                     Vec3 ( x.y, y.y, z.y ),
                     Vec3 ( x.z, y.z, z.z ) );
    }

    inline Dir GetBasisX() const { return Dir ( x.x, y.x, z.x ); }
    inline Dir GetBasisY() const { return Dir ( x.y, y.y, z.y ); }
    inline Dir GetBasisZ() const { return Dir ( x.z, y.z, z.z ); }
    inline void SetBasisX ( Dir in ) { x.x = in.x; y.x = in.y; z.x = in.z; }
    inline void SetBasisY ( Dir in ) { x.y = in.x; y.y = in.y; z.y = in.z; }
    inline void SetBasisZ ( Dir in ) { x.z = in.x; y.z = in.y; z.z = in.z; }

    inline Rot GetNormalised() const
    {
        return Rot::MakeFromBasis ( this->GetBasisX().GetNormalise(), 
                                    this->GetBasisY().GetNormalise(), 
                                    this->GetBasisZ().GetNormalise() );
    }

    // Constants.
    static Rot identity;

    // Conversions.
    inline float* AsFloatPtr() { return &x.x; }
    inline Mat44 ToMat44() const { return Mat44 ( Vec4 ( x.x, x.y, x.z, 0.0f ),
                                                  Vec4 ( y.x, y.y, y.z, 0.0f ),
                                                  Vec4 ( z.x, z.y, z.z, 0.0f ),
                                                  Vec4 ( 0.0f, 0.0f, 0.0f, 1.0f ) ); }
};

class Qot // Rotation as a quaternion. "Rot" -> "Qot". Chuckle!
{
public:
    // WARNING this is assumed normalized. If it is not, odd things will happen.
    Quat q;

    inline Qot() {}
    inline Qot ( float x, float y, float z, float w ) : q(x, y, z, w) {}
    inline Qot ( Quat q ) : q(q) {}
    inline static Qot FromFloatPtr ( float const* ptr ) { return Qot ( Quat::FromFloatPtr ( ptr ) ); }

    // Constants.
    static Qot identity;

};

class Orn // Orientation. A Rot and a Pos, stored as a compact mat3x4
{
public:
    // WARNING - these are ROW vectors, and do not "mean" very much.
    // Specifically, they are NOT the x/y/z basis vectors. Use GetBasisX/Y/Z to get those.
    // WARNING the "left hand" mat3x3 is assumed orthonormalized. If it is not, odd things will happen.
    Vec4 x, y, z;

    inline Orn() {}
    inline Orn ( Vec4 x, Vec4 y, Vec4 z ) : x(x), y(y), z(z) {}
    inline Orn ( Rot const &rot, Dir const &dir ) : x ( rot.x.x, rot.x.y, rot.x.z, dir.x ),
                                                    y ( rot.y.x, rot.y.y, rot.y.z, dir.y ),
                                                    z ( rot.z.x, rot.z.y, rot.z.z, dir.z ) {}
    inline static Orn FromFloatPtr ( float const* ptr ) { return Orn ( Vec4::FromFloatPtr ( ptr + 0 ),
                                                                       Vec4::FromFloatPtr ( ptr + 4 ),
                                                                       Vec4::FromFloatPtr ( ptr + 8 ) ); }

    inline Dir GetBasisX() const { return Dir ( x.x, y.x, z.x ); }
    inline Dir GetBasisY() const { return Dir ( x.y, y.y, z.y ); }
    inline Dir GetBasisZ() const { return Dir ( x.z, y.z, z.z ); }
    inline Dir GetBasisW() const { return Dir ( x.w, y.w, z.w ); }
    inline Rot GetRot() const { return Rot ( Vec3 ( x.x, x.y, x.z ),
                                             Vec3 ( y.x, y.y, y.z ),
                                             Vec3 ( z.x, z.y, z.z ) ); }
    inline void SetBasisX ( Dir in ) { x.x = in.x; y.x = in.y; z.x = in.z; }
    inline void SetBasisY ( Dir in ) { x.y = in.x; y.y = in.y; z.y = in.z; }
    inline void SetBasisZ ( Dir in ) { x.z = in.x; y.z = in.y; z.z = in.z; }
    inline void SetBasisW ( Dir in ) { x.w = in.x; y.w = in.y; z.w = in.z; }
    inline void SetRot ( Rot in ) { x.x = in.x.x; x.y = in.x.y; x.z = in.x.z;
                                    y.x = in.y.x; y.y = in.y.y; y.z = in.y.z;
                                    z.x = in.z.x; z.y = in.z.y; z.z = in.z.z; }

    // This relies on the inverse of the upper-left 3x3 being the transpose!
    inline Orn MakePseudoInverse() const
    {
        Rot result3x3 ( GetBasisX(), GetBasisY(), GetBasisZ() );
        Dir resultOffset = result3x3 * GetBasisW();
        return Orn ( result3x3, resultOffset );
    }

    inline Dir operator* ( Dir other ) const
    {
        Dir result;
        // Not "Dir" multiplication does not include the positional offset.
        result.x = (x.x * other.x) + (x.y * other.y) + (x.z * other.z);
        result.y = (y.x * other.x) + (y.y * other.y) + (y.z * other.z);
        result.z = (z.x * other.x) + (z.y * other.y) + (z.z * other.z);
        return result;
    }

    inline Pos operator* ( Pos other ) const
    {
        Pos result;
        result.x = (x.x * other.x) + (x.y * other.y) + (x.z * other.z) + x.w;
        result.y = (y.x * other.x) + (y.y * other.y) + (y.z * other.z) + y.w;
        result.z = (z.x * other.x) + (z.y * other.y) + (z.z * other.z) + z.w;
        return result;
    }

    inline Orn operator* ( Orn other ) const
    {
        Orn result;
        result.x.x = (x.x * other.x.x) + (x.y * other.y.x) + (x.z * other.z.x);
        result.x.y = (x.x * other.x.y) + (x.y * other.y.y) + (x.z * other.z.y);
        result.x.z = (x.x * other.x.z) + (x.y * other.y.z) + (x.z * other.z.z);
        result.x.w = (x.x * other.x.w) + (x.y * other.y.w) + (x.z * other.z.w) + x.w;

        result.y.x = (y.x * other.x.x) + (y.y * other.y.x) + (y.z * other.z.x);
        result.y.y = (y.x * other.x.y) + (y.y * other.y.y) + (y.z * other.z.y);
        result.y.z = (y.x * other.x.z) + (y.y * other.y.z) + (y.z * other.z.z);
        result.y.w = (y.x * other.x.w) + (y.y * other.y.w) + (y.z * other.z.w) + y.w;

        result.z.x = (z.x * other.x.x) + (z.y * other.y.x) + (z.z * other.z.x);
        result.z.y = (z.x * other.x.y) + (z.y * other.y.y) + (z.z * other.z.y);
        result.z.z = (z.x * other.x.z) + (z.y * other.y.z) + (z.z * other.z.z);
        result.z.w = (z.x * other.x.w) + (z.y * other.y.w) + (z.z * other.z.w) + z.w;

        return result;
    }

    // Scalar multiply and componentwise add are very odd operations, but used for animation, despite the hackiness.
    inline Orn ScalarMultiply ( float scale ) const
    {
        Orn result;
        result.x = x * scale;
        result.y = y * scale;
        result.z = z * scale;
        return result;
    }

    inline void ComponentwiseInc ( Orn other )
    {
        x += other.x;
        y += other.y;
        z += other.z;
    }

    // Deliberately no identity/zero for Orientations.

    // Conversions.
    inline float* AsFloatPtr() { return &x.x; }
    inline Mat44 ToMat44() const { return Mat44 ( x, y, z, Vec4 ( 0.0f, 0.0f, 0.0f, 1.0f ) ); }
};

class Qrn // Orientation, but using a quaternion. "Orn" -> "Qrn". Chuckle!
{
public:
    Qot q;
    Pos p;

    inline Qrn() {}
    inline Qrn ( Qot q, Pos p ) : q(q), p(p) {}

    // Deliberately no identity/zero for Orientations.

    // Conversions.
};



// Functions declared earlier that need to wait for everything to be defined first.

inline Dir Vec3::ToDir() const // TODO: convert to AsDir?
{
    return ( Dir ( x, y, z ) );
}

inline Pos Vec3::ToPos() const // TODO: convert to AsPos?
{
    return ( Pos ( x, y, z ) );
}

inline Vec4 Vec3::ToVec4 ( float w ) const
{
    return Vec4 ( x, y, z, w );
}

inline Vec3 Vec4::ToVec3() const
{
    return Vec3 ( x, y, z );
}

inline Dir Vec4::ToDir() const // TODO: convert to AsDir?
{
    ASSERT ( w == 0.0f );
    return ( Dir ( x, y, z ) );
}

inline Pos Vec4::ToPos() const // TODO: convert to AsPos?
{
    ASSERT ( w == 1.0f );
    return ( Pos ( x, y, z ) );
}

inline Quat Vec4::ToQuat() const // TODO: convert to AsQuat?
{
    return ( Quat ( x, y, z, w ) );
}

inline Rot Mat44::ToRot() const
{
    ASSERT ( x.w == 0.0f );
    ASSERT ( y.w == 0.0f );
    ASSERT ( z.w == 0.0f );
    ASSERT ( w.x == 0.0f );
    ASSERT ( w.y == 0.0f );
    ASSERT ( w.z == 0.0f );
    ASSERT ( w.w == 1.0f );
    return GetRot();
}

inline Rot Mat44::GetRot() const
{
    return Rot ( Vec3 ( x.x, x.y, x.z ),
                 Vec3 ( y.x, y.y, y.z ),
                 Vec3 ( z.x, z.y, z.z ) );
}

inline Orn Mat44::ToOrn() const // TODO: convert to AsOrn?
{
    ASSERT ( w.x == 0.0f );
    ASSERT ( w.y == 0.0f );
    ASSERT ( w.z == 0.0f );
    ASSERT ( w.w == 1.0f );
    return Orn ( x, y, z );
}


// WorldPosition class.
//
// Using floats for world positions is a terrible idea!
// 
// 24 bits of precision means that if an object is 17km from the origin, then millimeters don't exist.
// You might think this is acceptable, but think about two fighter jets side-by-side flying at Mach 1.
// In less than a minute of flying, they are 17km away from the origin. When you look from one to the
// other, they're wobbling strangely!
// 
// Worse still, you won't see this in your small test levels, where everything is near the origin.
// It's only when you make real larger areas that you will problems, but only at the edges.
// So debugging this is a nightmare. You really want consistent precision everywhere.
//
// The solution is simple - use int64s. Then you don't care where the world origin is -
// the precision is the same wherever you are in space.
// 
// For more, see https://tomforsyth1000.github.io/blog.wiki.html#%5B%5BA%20matter%20of%20precision%5D%5D 
//
// Note that asking "where am I" is not very useful (apart from save/load). What does zero even mean?
// Everything should be asking "where am I relative to this other thing". This is a very useful
// thing to ask, since the asnwer (0,0,0) means a useful thing - you have collided!

class WorldPosition
{
private:
    // This scale is arbitrary, but this chosen one means the top 32 bits can be read
    // as almost kilometers, so the largest distance is 4,294,967,296 km, which is roughly
    // the average distance of Neptune from the Sun. The smallest distance is 233 nanometers,
    // which should be precise enough for most purposes.
    // 
    // To make the conversion exact, the multiplier is 1024.0f, not 1000.0f. Kibimeters!
    float const FromMeters = 1024.0f * (float)(1llu<<32);
    float const ToMeters = 1.0f / FromMeters;

public:
    WorldPosition() : x(0), y(0), z(0) {}

    void Init()
    {
        x = 0;
        y = 0;
        z = 0;
    }

    void operator= (WorldPosition other)
    {
        x = other.x;
        y = other.y;
        z = other.z;
    }

    bool operator== ( WorldPosition other )
    {
        return ( ( x == other.x ) && ( y == other.y ) && ( z == other.z ) );
    }

    // Use like:
    //
    // Dir objectAFromObjectB = objectA.worldPos.MetersFrom ( objectB.worldPos );
    Dir MetersFrom ( WorldPosition const &other ) const
    {
        Dir result;
        sint64 deltaX = x - other.x;
        sint64 deltaY = y - other.y;
        sint64 deltaZ = z - other.z;
        result.x = ToMeters * (float)deltaX;
        result.y = ToMeters * (float)deltaY;
        result.z = ToMeters * (float)deltaZ;
        return result;
    }

    [[nodiscard]] WorldPosition AddMeters ( Dir offset ) const
    {
        WorldPosition result (*this);
        result.IncMeters ( offset );
        return result;
    }

    void IncMeters ( Dir offset )
    {
        x += (sint64)floorf ( FromMeters * offset.x + 0.5f );
        y += (sint64)floorf ( FromMeters * offset.y + 0.5f );
        z += (sint64)floorf ( FromMeters * offset.z + 0.5f );
    }

private:
    sint64 x, y, z;
};


// A class with a WorldPosition and a Rot together.
// 
// WorldPosition comes from the Yak-Shiv misc library, so make sure you put that first in your include lists.

class WorldOrientation
{
public:
    WorldPosition posWorld;
    Rot worldFromObject;

    // Use like:
    // 
    // Orn objectAFromObjectB = objectA.worldOrn.GetFrom ( objectB.worldOrn );
    //
    // This can be used to transform points in ObjectB space into points in ObjectA space, e.g. to aim ObjectA's turret at them.
    // But the most common use is if objectA is the camera in rendering - to transform points in ObjectB space to camera space.
    Orn GetFrom ( WorldOrientation const &other ) const
    {
        Rot thisObjectFromWorld = this->worldFromObject.GetTranspose();
        Dir thisFromOtherDir = thisObjectFromWorld * other.posWorld.MetersFrom ( this->posWorld );
        Rot thisFromOtherOrn = thisObjectFromWorld * other.worldFromObject;
        Orn result ( thisFromOtherOrn, thisFromOtherDir );
        return result;
    }
};


// A class that separates simulation update speed from rendering speed.
//
// It stores the last two WorldOrientations, along with the timestamps when they were generated.
// The renderer queries them with another timestamp, and the two are interpolated (or extrapolated!)
// to produce the rendered position.

class WorldOrientationAsync
{
private:
    // The last simulation state.
    WorldOrientation orn;
    NtpTimeStamp time;
    bool teleported = false; // Set to true to disable interpolation.

    // The deltas to the previous simulation step.
    Rot deltaOrn;
    Dir deltaPos;
    float deltaSeconds = 0.0f;

public:

    // Optionally returns how many seconds since the last sim for this object.
    WorldOrientation const& StartSimulationUpdate ( float *pDeltaSecondsResult, NtpTimeStamp newSimTime )
    {
        if ( pDeltaSecondsResult != nullptr )
        {
            *pDeltaSecondsResult = newSimTime.SecondsSince ( time );
        }
        teleported = false;
        return orn;
    }

    void FinishSimulationUpdate ( NtpTimeStamp newSimTime, WorldOrientation newOrn, bool teleportedIn )
    {
        deltaSeconds = newSimTime.SecondsSince ( time );
        if ( teleportedIn )
        {
            deltaOrn = Rot ( Vec3 ( 0.0f, 0.0f, 0.0f ),
                             Vec3 ( 0.0f, 0.0f, 0.0f ),
                             Vec3 ( 0.0f, 0.0f, 0.0f ) );
            deltaPos = Dir::zero;
        }
        else
        {
            // This is a very dirty hack. We take the componentwise delta, which "means" nothing,
            // but for small rotations it's good enough to interpolate/extrapolate with.
            deltaOrn = Rot ( newOrn.worldFromObject.x - orn.worldFromObject.x, 
                             newOrn.worldFromObject.y - orn.worldFromObject.y, 
                             newOrn.worldFromObject.z - orn.worldFromObject.z );
            deltaPos = newOrn.posWorld.MetersFrom ( orn.posWorld );
        }
        this->teleported = teleportedIn;
        orn = newOrn;
        time = newSimTime;
    }

    // renderTime can be before or after "time" depending on whether you are interpolating or extrapolating.
    WorldOrientation InterpolateRenderStat ( NtpTimeStamp renderTime ) const
    {
        if ( teleported )
        {
            // No interpolation, just use the latest.
            return orn;
        }

        float secondsRenderStep = -time.SecondsSince ( renderTime );
        float lerpFactor = secondsRenderStep / deltaSeconds;

        // Linearly apply the deltas to the components.
        // This is a dirty hack, but for small rotations it should be fine.
        Rot lerpedRot = Rot ( Vec3 ( orn.worldFromObject.x + deltaOrn.x * lerpFactor ),
                              Vec3 ( orn.worldFromObject.y + deltaOrn.y * lerpFactor ),
                              Vec3 ( orn.worldFromObject.z + deltaOrn.z * lerpFactor ) );
        WorldOrientation result;
        result.worldFromObject = lerpedRot.GetNormalised();
        result.posWorld = orn.posWorld.AddMeters ( deltaPos * lerpFactor );
        return result;
    }
};


// Projection matrix routines.
//
// Most tutorials and libraries focus on the traditional projection matrices used in the 1980s.
// These are obsolete! We have floating-point Z buffers these days, and we understand about
// infinite far clip planes.
//
// Brief summary:
// 1. Don't use stencil any more. Many GPUs emulate them anyway. Instead use real rendertargets and all that good stuff.
// 2. Don't use integer Z (DXGI_FORMAT_D24_UNORM_S8_UINT), use float32 Z (DXGI_FORMAT_D32_FLOAT).
// 3. Use reverse Z, so that 1.0 is nearby, 0.0 is in the distance. It gives more even precision in a float32 Z buffer.
// 4. Use an infinite far clip plane.
//
// If you don't know what some of the above means (and there's no reason you should) - don't worry about it!
// This library does all of those things for you.
// 
// When using Reverze Z, remember:
// 1. Clear Z to 0.0f, not 1.0f (ClearDepthStencilView)
// 2. Flip your Z test
// 
// Also, be careful - this is the Direct3D version that maps to [0,1]
// OpenGL needs to map to [-1,1] which doesn't work at all with
// reverse Z unless you use the ARB_clip_control/glClipControl to fix it.
//
// More info:
// https://www.reedbeta.com/blog/depth-precision-visualized/
// https://iolite-engine.com/blog_posts/reverse_z_cheatsheet

// Left handed: X=right, Y=up, Z=away
// Right handed: X=right, Y=up, Z=towards
// invTanHalfFovH = 1 / tan(Horizontal_FOV / 2)
Mat44 ProjectionMatrixInfFarClipReverseZDirect3D ( float invTanHalfFovH, float invTanHalfFovV, float zNear, bool rightHanded)
{
    float handedScale = rightHanded ? -1.0f : 1.0f;

    return Mat44 ( Vec4 ( invTanHalfFovH, 0.0f,           0.0f,        0.0f  ),
                   Vec4 ( 0.0f,           invTanHalfFovV, 0.0f,        0.0f  ),
                   Vec4 ( 0.0f,           0.0f,           0.0f,        zNear ),
                   Vec4 ( 0.0f,           0.0f,           handedScale, 0.0f  ) );
}

// Non-infinite far clip plane, but still reversed Z.
Mat44 ProjectionMatrixReverseZDirect3D ( float invTanHalfFovH, float invTanHalfFovV, float zNear, float zFar, bool rightHanded )
{
    float flipRatio = (zNear / (zNear - zFar));
    float handedScale = rightHanded ? -1.0f : 1.0f;

    return Mat44 ( Vec4 ( invTanHalfFovH, 0.0f,           0.0f,                    0.0f              ),
                   Vec4 ( 0.0f,           invTanHalfFovV, 0.0f,                    0.0f              ),
                   Vec4 ( 0.0f,           0.0f,           handedScale * flipRatio, -zFar * flipRatio ),
                   Vec4 ( 0.0f,           0.0f,           handedScale,             0.0f              ) );
}

// If you really really need it, or just for comparison, here is the standard Direct3D projection matrix. But there's really no advantage.
// Note this should match DirectX::XMMatrixPerspectiveFovRH/LH - the reason to use these is there's no reverse-Z or infinite-far options.
// Left handed: X=right, Y=up, Z=away
// Right handed: X=right, Y=up, Z=towards
// invTanHalfFovH = 1 / tan(Horizontal_FOV / 2)
// (the sharp-eyed will note that it is literally the above, but with "zNear" and "zFar" swapped - funny that!)
Mat44 ProjectionMatrixDirect3D ( float invTanHalfFovH, float invTanHalfFovV, float zNear, float zFar, bool rightHanded )
{
    float ratio = (zFar / (zFar - zNear));
    float handedScale = rightHanded ? -1.0f : 1.0f;

    return Mat44 ( Vec4 ( invTanHalfFovH, 0.0f,           0.0f,                0.0f           ),
                   Vec4 ( 0.0f,           invTanHalfFovV, 0.0f,                0.0f           ),
                   Vec4 ( 0.0f,           0.0f,           handedScale * ratio, -zNear * ratio ),
                   Vec4 ( 0.0f,           0.0f,           handedScale,         0.0f           ) );
}


#ifdef YAK_SHIV_MATRIX_IMPLEMENTATION

/*static*/ Vec2 Vec2::zero = { 0.0f, 0.0f };
/*static*/ Vec3 Vec3::zero = { 0.0f, 0.0f, 0.0f };
/*static*/ Vec4 Vec4::zero = { 0.0f, 0.0f, 0.0f, 0.0f, };
/*static*/ Mat44 Mat44::identity = { { 1.0f, 0.0f, 0.0f, 0.0f },
                                     { 0.0f, 1.0f, 0.0f, 0.0f },
                                     { 0.0f, 0.0f, 1.0f, 0.0f },
                                     { 0.0f, 0.0f, 0.0f, 1.0f } };
/*static*/ Quat Quat::identity = { 0.0f, 0.0f, 0.0f, 1.0f };
/*static*/ Dir Dir::zero = { 0.0f, 0.0f, 0.0f };
/*static*/ Rot Rot::identity = { Vec3 { 1.0f, 0.0f, 0.0f },
                                 Vec3 { 0.0f, 1.0f, 0.0f },
                                 Vec3 { 0.0f, 0.0f, 1.0f } };
/*static*/ Qot Qot::identity = { { 0.0f, 0.0f, 0.0f, 1.0f } };

/*static*/ Rot Rot::MakeRotateX ( float angleInRadians )
{
    // TODO: check signs.
    float sinAngle = sinf ( angleInRadians );
    float cosAngle = cosf ( angleInRadians );
    return Rot ( Vec3 ( 1.0f,  0.0f,      0.0f     ),
                 Vec3 ( 0.0f,  cosAngle,  sinAngle ),
                 Vec3 ( 0.0f, -sinAngle,  cosAngle ) );
}

/*static*/ Rot Rot::MakeRotateY ( float angleInRadians )
{
    // TODO: check signs.
    float sinAngle = sinf ( angleInRadians );
    float cosAngle = cosf ( angleInRadians );
    return Rot ( Vec3 (  cosAngle, 0.0f, -sinAngle ),
                 Vec3 (  0.0f,     1.0f,  0.0f     ),
                 Vec3 (  sinAngle, 0.0f,  cosAngle ) );
}

/*static*/ Rot Rot::MakeRotateZ ( float angleInRadians )
{
    // TODO: check signs.
    float sinAngle = sinf ( angleInRadians );
    float cosAngle = cosf ( angleInRadians );
    return Rot ( Vec3 (  cosAngle,  sinAngle, 0.0f ),
                 Vec3 ( -sinAngle,  cosAngle, 0.0f ),
                 Vec3 (  0.0f    ,  0.0f,     1.0f ) );
}

Mat44 Mat44::GetInverse() const
{
    // Thanks to ccVector library for this inverse function.
	float s[6];
	float c[6];

    Mat44 const &mat = *this; // just for readability;

	s[0] = mat.x.x * mat.y.y - mat.y.x * mat.x.y;
	s[1] = mat.x.x * mat.y.z - mat.y.x * mat.x.z;
	s[2] = mat.x.x * mat.y.w - mat.y.x * mat.x.w;
	s[3] = mat.x.y * mat.y.z - mat.y.y * mat.x.z;
	s[4] = mat.x.y * mat.y.w - mat.y.y * mat.x.w;
	s[5] = mat.x.z * mat.y.w - mat.y.z * mat.x.w;

	c[0] = mat.z.x * mat.w.y - mat.w.x * mat.z.y;
	c[1] = mat.z.x * mat.w.z - mat.w.x * mat.z.z;
	c[2] = mat.z.x * mat.w.w - mat.w.x * mat.z.w;
	c[3] = mat.z.y * mat.w.z - mat.w.y * mat.z.z;
	c[4] = mat.z.y * mat.w.w - mat.w.y * mat.z.w;
	c[5] = mat.z.z * mat.w.w - mat.w.z * mat.z.w;

	float det = s[0] * c[5] - s[1] * c[4] + s[2] * c[3] + s[3] * c[2] - s[4] * c[1] + s[5] * c[0];
	ASSERT(det != 0);
    float idet = 1.0f / det;

    Mat44 result = Mat44 (
        Vec4 ( ( mat.y.y * c[5] - mat.y.z * c[4] + mat.y.w * c[3]) * idet,
               (-mat.x.y * c[5] + mat.x.z * c[4] - mat.x.w * c[3]) * idet,
               ( mat.w.y * s[5] - mat.w.z * s[4] + mat.w.w * s[3]) * idet,
               (-mat.z.y * s[5] + mat.z.z * s[4] - mat.z.w * s[3]) * idet ),

        Vec4 ( (-mat.y.x * c[5] + mat.y.z * c[2] - mat.y.w * c[1]) * idet,
               ( mat.x.x * c[5] - mat.x.z * c[2] + mat.x.w * c[1]) * idet,
               (-mat.w.x * s[5] + mat.w.z * s[2] - mat.w.w * s[1]) * idet,
               ( mat.z.x * s[5] - mat.z.z * s[2] + mat.z.w * s[1]) * idet ),

        Vec4 ( ( mat.y.x * c[4] - mat.y.y * c[2] + mat.y.w * c[0]) * idet,
               (-mat.x.x * c[4] + mat.x.y * c[2] - mat.x.w * c[0]) * idet,
               ( mat.w.x * s[4] - mat.w.y * s[2] + mat.w.w * s[0]) * idet,
               (-mat.z.x * s[4] + mat.z.y * s[2] - mat.z.w * s[0]) * idet ),

        Vec4 ( (-mat.y.x * c[3] + mat.y.y * c[1] - mat.y.z * c[0]) * idet,
               ( mat.x.x * c[3] - mat.x.y * c[1] + mat.x.z * c[0]) * idet,
               (-mat.w.x * s[3] + mat.w.y * s[1] - mat.w.z * s[0]) * idet,
               ( mat.z.x * s[3] - mat.z.y * s[1] + mat.z.z * s[0]) * idet ) );

    return result;
}


#endif



