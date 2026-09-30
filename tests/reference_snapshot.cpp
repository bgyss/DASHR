#include "../demo/comparison_snapshot_format.h"
#include <cassert>
#include <iostream>
#include <limits>

int main(int argc, char** argv) {
    using namespace dashr_snapshot;
    Snapshot s;
    const float rows[16] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16};
    auto transposed=columns_from_rows(rows);
    assert(transposed[0][1]==5 && transposed[1][0]==2);
    const float camera_rows[16]={1,.25f,0,4,0,1,0,-2,0,0,1,7,0,0,0,1};
    s.uniforms.camera_from_object = columns_from_rows(camera_rows);
    s.uniforms.object_from_camera = {{{1,0,0,0},{-.25f,1,0,0},{0,0,1,0},{-4.5f,2,-7,1}}};
    s.uniforms.bones[1] = s.uniforms.bones[3];
    s.uniforms.bones[3] = {{{0,1,0,0},{-1,0,0,0},{0,0,1,0},{2,3,4,1}}};
    s.adapter = "GPU \"one\"\n\\";
    auto json = manifest(s);
    assert(json.find("\"schema\":\"dashr.reference.snapshot.v1\"") != std::string::npos);
    assert(json.find("\"camera_from_object\":[[1,0,0,0],[0.25,1,0,0],[0,0,1,0],[4,-2,7,1]]") != std::string::npos);
    assert(json.find("GPU \\\"one\\\"\\n\\\\") != std::string::npos);
    assert(json.find("\"control\":[0,0,10000,0]") != std::string::npos);
    assert(json.find("\"frame\":\"frame.png\"") != std::string::npos);
    if (argc == 2 && std::string(argv[1]) == "--json") { std::cout << json << '\n'; return 0; }
    s.uniforms.bones[2][1][3] = std::numeric_limits<float>::infinity();
    bool rejected = false;
    try { manifest(s); } catch (const std::invalid_argument&) { rejected = true; }
    assert(rejected);
    s.uniforms.bones[2][1][3] = 0;
    s.time = std::numeric_limits<float>::quiet_NaN();
    rejected = false;
    try { manifest(s); } catch (const std::invalid_argument&) { rejected = true; }
    assert(rejected);
    std::cout << "reference snapshot formatter tests passed\n";
}
