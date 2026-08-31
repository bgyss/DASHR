
// ConstantBufferStruct needs to match between the C version and the shader definition in ConstantBuffer.hlsl
// Must also be a multiple of 16 bytes, and vec2/3/4 must also be aligned.

cbuffer ConstantBufferStruct
{
	row_major float4x4 projectionFromCameraMatrix;
	row_major float4x4 cameraFromObjectMatrix;
	row_major float4x4 objectFromCameraMatrix;

	float HeightScale;
	float HeightOffset;
	float StepSize;
	float StepScale;

	row_major float4x4 BoneFromObject[4];

	float3 SunDirInObject;
	float SurfaceFromObjectTextureSize;

    int DebugMode;
    int LightingMode;
    int DistortionMode;
    int MaxSteps;
	
	float DampingFactor1;
	float DampingFactor2;
	float DampingFactor3;
	float HeightExtraMeshExtrude;

    float DeltaUVStep;
    float ShadowAcneScaler;
    float IndirectLighting;
    float HeightNormalsScale;

	int DebugIterationsAfterTeleport;
	int Padding1;
	int Padding2;
	int Padding3;

};

