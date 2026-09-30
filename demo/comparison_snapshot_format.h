// Portable snapshot schema. Mat44 inputs are rows; replay matrices are columns.
#pragma once
#include <array>
#include <cmath>
#include <iomanip>
#include <limits>
#include <locale>
#include <sstream>
#include <stdexcept>
#include <string>

namespace dashr_snapshot {
using Matrix = std::array<std::array<float, 4>, 4>;
inline Matrix columns_from_rows(const float* rows) {
    Matrix result{};
    for (int c = 0; c < 4; ++c) for (int r = 0; r < 4; ++r) result[c][r] = rows[r * 4 + c];
    return result;
}
inline Matrix identity() { return {{{1,0,0,0},{0,1,0,0},{0,0,1,0},{0,0,0,1}}}; }
struct Uniforms {
    Matrix projection = identity(), camera_from_object = identity(), object_from_camera = identity();
    std::array<float, 4> height_step{1,0,0.01f,1};
    std::array<Matrix, 4> bones{identity(),identity(),identity(),identity()};
    std::array<float, 4> sun_atlas{0,1,0,256};
    std::array<int, 4> modes{0,4,1,-1};
    std::array<float, 4> damping_extrusion{1,0,1.5f,0.25f}, lighting{0.002f,0.01f,0.2f,1.5f};
    std::array<int, 4> control{0,0,10000,0};
};
static_assert(sizeof(Uniforms) == 544, "Replay uniform layout must remain 544 bytes");
struct Snapshot {
    std::string status = "captured", error, adapter = "unknown", feature_level = "unknown";
    std::string mesh = "tube";
    int around = 8, long_segments = 8, atlas = 256, width = 640, height = 480, texture_set = 0;
    float length = 4, radius = 1, thickness = 1, time = 10, bone_time2 = 10, animation_period = 12, animation_amount = 1;
    std::array<float,3> camera{0,0,8}, target{0,0,0}, background{0.45f,0.55f,0.60f};
    float near_plane = 0.1f, fov_y = 44.998673f, sun_time = 8, sun_period = 19, sun_elevation = 0.5f;
    bool overlay = false, flood_fill = true;
    int alpha_mode = 0;
    Uniforms uniforms;
};
class Json {
    std::ostringstream out;
public:
    Json() { out.imbue(std::locale::classic()); out << std::setprecision(std::numeric_limits<float>::max_digits10); }
    void raw(const char* s) { out << s; }
    void value(const std::string& s) {
        out << '"';
        for (unsigned char c : s) {
            switch (c) {
                case '"': out << "\\\""; break;
                case '\\': out << "\\\\"; break;
                case '\n': out << "\\n"; break;
                case '\r': out << "\\r"; break;
                case '\t': out << "\\t"; break;
                default:
                    if (c < 32) { const char* hex = "0123456789abcdef"; out << "\\u00" << hex[c >> 4] << hex[c & 15]; }
                    else out << c;
            }
        }
        out << '"';
    }
    void value(float f) { if (!std::isfinite(f)) throw std::invalid_argument("snapshot contains a non-finite number"); out << f; }
    void value(int n) { out << n; }
    void value(bool b) { out << (b ? "true" : "false"); }
    template<class T, size_t N> void value(const std::array<T,N>& a) { out << '['; bool first=true; for (const auto& v:a) { if (!first) out << ','; first=false; value(v); } out << ']'; }
    template<class T> void field(const char* key, const T& v) { value(std::string(key)); raw(":"); value(v); raw(","); }
    std::string str() { return out.str(); }
};
inline std::string manifest(const Snapshot& s) {
    Json j;
    j.raw("{"); j.field("schema",std::string("dashr.reference.snapshot.v1")); j.field("status",s.status);
    j.field("renderer",std::string("d3d11")); j.field("width",s.width); j.field("height",s.height);
    j.field("frame",std::string("frame.png")); j.field("overlay",s.overlay ? std::string("overlay.png") : std::string()); j.field("error",s.error);
    j.raw("\"source\":{"); j.field("adapter",s.adapter); j.field("feature_level",s.feature_level);
    j.field("bone_time",s.time); j.field("bone_time2",s.bone_time2); j.field("bone_period",s.animation_period);
    j.field("bone_period2",s.animation_period * 0.763f); j.field("flood_fill",s.flood_fill); j.field("alpha_mode",s.alpha_mode);
    j.field("animation_frozen",true); j.field("sun_frozen",true);
    j.field("color",std::string("UNORM RGBA/BGRA8 bytes; no sRGB view conversion; PNG without color transform"));
    j.field("samplers",std::string("linear clamp seam distance and materials; nearest destination UV"));
    j.field("depth",std::string("D32_FLOAT reverse Z, clear=0, infinite far"));
    j.raw("\"uniform_layout\":\"544 bytes, column-major replay matrices; source Mat44 rows transposed\"},\"settings\":{");
    j.field("mesh",s.mesh); j.field("around",s.around); j.field("long",s.long_segments); j.field("length",s.length); j.field("radius",s.radius); j.field("thickness",s.thickness);
    j.field("atlas",s.atlas); j.field("width",s.width); j.field("height",s.height); j.field("texture_set",s.texture_set);
    j.field("time",s.time); j.field("animation_amount",s.animation_amount); j.field("camera",s.camera); j.field("target",s.target);
    j.field("near",s.near_plane); j.field("fov_y",s.fov_y); j.field("background",s.background);
    j.field("sun_time",s.sun_time); j.field("sun_period",s.sun_period); j.field("sun_elevation",s.sun_elevation);
    const auto& u=s.uniforms;
    j.field("height_scale",u.height_step[0]); j.field("height_offset",u.height_step[1]); j.field("step_size",u.height_step[2]); j.field("step_scale",u.height_step[3]);
    j.field("debug",u.modes[0]); j.field("lighting_mode",u.modes[1]); j.field("distortion",u.modes[2]); j.field("debug_max_steps",u.modes[3]);
    j.field("damping",std::array<float,3>{u.damping_extrusion[0],u.damping_extrusion[1],u.damping_extrusion[2]}); j.field("extra_extrusion",u.damping_extrusion[3]);
    j.field("delta_uv",u.lighting[0]); j.field("shadow_acne",u.lighting[1]); j.field("indirect",u.lighting[2]); j.field("normal_scale",u.lighting[3]);
    j.field("teleport_iterations",u.control[0]); j.field("step_budget",u.control[2]); j.field("hit_depth",false);
    j.raw("\"uniform_override\":{");
    j.field("projection",u.projection); j.field("camera_from_object",u.camera_from_object); j.field("object_from_camera",u.object_from_camera);
    j.field("height_step",u.height_step); j.field("bones",u.bones); j.field("sun_atlas",u.sun_atlas); j.field("modes",u.modes);
    j.field("damping_extrusion",u.damping_extrusion); j.field("lighting",u.lighting); j.raw("\"control\":"); j.value(u.control);
    j.raw("}}}\n"); return j.str();
}
} // namespace dashr_snapshot
